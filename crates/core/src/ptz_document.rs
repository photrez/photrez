// SPDX-License-Identifier: AGPL-3.0-or-later
// The `.ptz` `document.json` payload: the typed document Rust WRITES on save.
//
// `canonical_model.rs` holds the document CONTENT model (layers, transforms,
// text, adjustments). This module holds everything the file needs on top of
// that: the `photrez-ptz` / version header, the session fields the loader
// restores, and the two per-layer bitmap keys that must stay null.
//
// PRODUCTION PATH: `apps/desktop/src-tauri/src/save_stream.rs`
// (`save_project_streaming_begin`) parses the host's document into
// `PtzDocument`, serializes it HERE, and writes the bytes into the ZIP. The
// host no longer builds the save payload itself.
//
// WHY THE SESSION FIELDS LIVE HERE AND NOT IN `canonical_model.rs`:
// `activeLayerId` / `viewport` / `dirty` are editor state, not document
// content, so they are deliberately not part of `CanonicalDocument`. They are
// still part of the FILE, because `editorOpenImage.ts` `loadProjectFile`
// restores the viewport from the saved document. Keeping them in a
// file-payload type (not the content model) is what lets the content model
// stay free of view state without the file losing it.
//
// BITMAP KEYS: `imageBitmap` and `baseImageBitmap` are always written as
// `null`. Pixels live in the ZIP as `layers/<id>.png`, never in the JSON.
// They are emitted here rather than modelled as fields because the host holds
// live `ImageBitmap` objects there, which are not JSON and must not survive
// the IPC boundary.

#[cfg(test)]
use crate::canonical_model::CanonicalDocument;
use crate::canonical_model::{CanonicalLayer, SelectionState};
use serde::{Deserialize, Serialize};

/// The `.ptz` container format marker. `format: "photrez-ptz"` in `document.json`.
const PTZ_FORMAT: &str = "photrez-ptz";

/// The `document.json` schema version this build writes. v3 carries text and
/// shape layer metadata (they ride the layer spread). This is the version the
/// loader in `editorOpenImage.ts` reads without a "saved by a newer version"
/// warning, so it must NOT be raised without teaching that loader first.
///
/// There is a second `.ptz` writer in this crate:
/// `CanonicalDocument::to_ptz_document_json` in `canonical_model.rs` emits
/// `version: 4` and carries neither `viewport` nor `activeLayerId`, so a file
/// it wrote would toast on every open and reset the view. It is fenced to
/// `#[cfg(test)]`. Deleting it is a PENDING DECISION, deliberately not taken
/// in the change that added this module. Do not wire it into a save path.
const PTZ_VERSION: u32 = 3;

/// types.ts:122-127 - ViewportState (camelCase on the wire).
///
/// Defaults to identity at 100% zoom so a hand-edited or pre-viewport file
/// loads at a usable zoom instead of an all-`undefined` viewport, which the
/// renderer would turn into NaN geometry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewportState {
    pub pan_x: f64,
    pub pan_y: f64,
    pub zoom: f64,
    pub rotation: f64,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
            rotation: 0.0,
        }
    }
}

/// The `.ptz` `document.json` payload as written to disk.
///
/// `format` / `version` and the per-layer bitmap keys are not fields: they are
/// appended by `to_json`, which is the only place the on-disk header is built.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtzDocument {
    pub id: String,
    pub name: String,
    pub width: f64,
    pub height: f64,
    pub layers: Vec<CanonicalLayer>,
    /// `None` (written `null`) when no layer is active. Restored by the loader.
    #[serde(default)]
    pub active_layer_id: Option<String>,
    #[serde(default)]
    pub selection: Option<SelectionState>,
    /// Restored by the loader (`restoreViewport: true` on project open).
    #[serde(default)]
    pub viewport: ViewportState,
    /// Save-state flag. The loader clears it after restore, so it is written
    /// for fidelity with the previous writer rather than load-bearing.
    #[serde(default)]
    pub dirty: bool,
}

impl PtzDocument {
    /// Parse a `document.json` payload into the typed file model.
    ///
    /// Unknown keys are ignored (a newer build's extra fields), and absent
    /// optional fields take their defaults, so a v1/v2/v3 file all parse.
    /// Malformed JSON or an out-of-contract field type returns `Err` -- the
    /// save then fails before any file is created rather than writing a
    /// document that cannot be read back.
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    /// Serialize to the exact `document.json` bytes written into the ZIP.
    ///
    /// Appends the `photrez-ptz` / version header and the two per-layer
    /// bitmap keys. Both bitmap keys are written as a constant `null` and any
    /// value the host sent for them is discarded: the host holds live
    /// `ImageBitmap` objects there, and pixels belong in the ZIP
    /// (`layers/<id>.png`), never in the JSON. Writing the constant also stops
    /// a live bitmap from reaching `JSON.stringify`, which would serialize it
    /// to `{}` and make the loader treat it as a usable base image.
    pub fn to_json(&self) -> Result<String, String> {
        let mut value = serde_json::to_value(self).map_err(|e| e.to_string())?;
        if let serde_json::Value::Object(ref mut map) = value {
            map.insert(
                "format".to_string(),
                serde_json::Value::String(PTZ_FORMAT.to_string()),
            );
            map.insert(
                "version".to_string(),
                serde_json::Value::Number(serde_json::Number::from(PTZ_VERSION)),
            );
            if let Some(serde_json::Value::Array(ref mut layers)) = map.get_mut("layers") {
                for layer in layers.iter_mut() {
                    if let serde_json::Value::Object(ref mut layer_map) = layer {
                        layer_map.insert("imageBitmap".to_string(), serde_json::Value::Null);
                        layer_map.insert("baseImageBitmap".to_string(), serde_json::Value::Null);
                    }
                }
            }
        }
        serde_json::to_string(&value).map_err(|e| e.to_string())
    }

    /// The document CONTENT, dropping the session/transport fields.
    #[cfg(test)]
    pub fn to_canonical(&self) -> CanonicalDocument {
        CanonicalDocument {
            id: self.id.clone(),
            name: self.name.clone(),
            width: self.width,
            height: self.height,
            layers: self.layers.clone(),
            selection: self.selection.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical_model::SelectionShape;

    /// Fixtures live in `ptz_fixtures` so the core writer tests and the desktop
    /// command tests (`save_stream.rs`) run against the SAME bytes and cannot
    /// drift apart.
    use crate::ptz_fixtures::{
        LEGACY_TEXT_LAYER_MODEL, MIXED_LAYER_TYPES_MODEL, REAL_DUMPED_MODEL,
    };

    fn parse_fixture() -> PtzDocument {
        PtzDocument::from_json(REAL_DUMPED_MODEL).expect("real dumped model parses")
    }

    fn round_trip(json: &str) -> PtzDocument {
        PtzDocument::from_json(
            &PtzDocument::from_json(json)
                .expect("parse")
                .to_json()
                .expect("serialize"),
        )
        .expect("written payload re-parses")
    }

    fn layer_by_name<'a>(doc: &'a PtzDocument, name: &str) -> &'a CanonicalLayer {
        doc.layers
            .iter()
            .find(|l| l.name == name)
            .unwrap_or_else(|| panic!("layer {} present", name))
    }

    /// Compare two JSON values structurally, treating numbers by VALUE rather
    /// than by representation. A float written as `300.0` and one written as
    /// `300` are the same document number, but `serde_json::Number`'s
    /// `PartialEq` distinguishes the two spellings, so a plain `==` reports a
    /// difference where none exists. Recurses, because the fields being
    /// compared (`selection`, `transform`) are nested objects.
    fn same_json_value(a: &serde_json::Value, b: &serde_json::Value) -> bool {
        match (a, b) {
            (serde_json::Value::Number(x), serde_json::Value::Number(y)) => {
                x.as_f64() == y.as_f64()
            }
            (serde_json::Value::Object(x), serde_json::Value::Object(y)) => {
                x.len() == y.len()
                    && x.iter()
                        .all(|(k, v)| y.get(k).map(|w| same_json_value(v, w)).unwrap_or(false))
            }
            (serde_json::Value::Array(x), serde_json::Value::Array(y)) => {
                x.len() == y.len() && x.iter().zip(y.iter()).all(|(v, w)| same_json_value(v, w))
            }
            _ => a == b,
        }
    }

    /// The exact key set the writer must emit for a document. Anything added
    /// or missing is a contract change, so this is asserted as a SET, not by
    /// spot-checking listed keys: `serde_json::Value::Index` yields `Null` for a
    /// missing key, so a per-key loop cannot tell "absent" from "null".
    const DOC_KEYS: &[&str] = &[
        "activeLayerId",
        "dirty",
        "format",
        "height",
        "id",
        "layers",
        "name",
        "selection",
        "version",
        "viewport",
        "width",
    ];

    /// The exact key set the writer must emit for every layer. `resourceId` is
    /// always present and always `null` (a `ResourceId` is a session-scoped
    /// token handle; see `to_json`). The nested payload keys are listed here
    /// because a writer that silently dropped one could not be caught by a
    /// raster-only fixture.
    const LAYER_KEYS: &[&str] = &[
        "baseImageBitmap",
        "basicAdjustment",
        "blendMode",
        "hasAdjustments",
        "height",
        "id",
        "imageBitmap",
        "isBackground",
        "lockPosition",
        "lockRotation",
        "lockTransparency",
        "locked",
        "name",
        "opacity",
        "resourceId",
        "shapeParams",
        "textData",
        "transform",
        "type",
        "visible",
        "width",
    ];

    /// `shapeParams` sub-keys, asserted on the shape layer.
    const SHAPE_PARAM_KEYS: &[&str] = &[
        "arrowHead",
        "fill",
        "height",
        "kind",
        "radius",
        "stroke",
        "width",
    ];

    /// `textData` sub-keys, asserted on the text layer. Sorted, because
    /// `assert_exact_keys` compares sorted key lists.
    const TEXT_DATA_KEYS: &[&str] = &[
        "align",
        "boxHeight",
        "boxMode",
        "boxWidth",
        "color",
        "content",
        "fontFamily",
        "fontSize",
        "fontStyle",
        "fontWeight",
        "letterSpacing",
        "lineHeight",
        "stroke",
        "strikethrough",
        "underline",
        "uppercase",
    ];

    /// Sorted key list of a JSON object, for set comparison.
    fn keys_of(value: &serde_json::Value) -> Vec<String> {
        let mut keys: Vec<String> = value.as_object().expect("object").keys().cloned().collect();
        keys.sort();
        keys
    }

    /// Assert `value` is an object whose key set is exactly `expected`.
    /// Catches an ADDED key and a MISSING key; `Index` cannot catch either.
    /// `expected` is sorted here so the literal lists above can stay in
    /// declaration order and cannot be silently mis-sorted.
    fn assert_exact_keys(value: &serde_json::Value, expected: &[&str], what: &str) {
        let mut want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        want.sort();
        assert_eq!(
            keys_of(value),
            want,
            "{} key set changed (added or missing key)",
            what
        );
    }

    // ── The production round trip: real dumped file -> write -> read ──

    #[test]
    fn real_dumped_model_survives_the_production_writer() {
        let original = parse_fixture();
        let reread = round_trip(REAL_DUMPED_MODEL);

        // Document-level content.
        assert_eq!(reread.id, "dump-1788754352188");
        assert_eq!(reread.name, "Dump");
        assert_eq!(reread.width, 300.0);
        assert_eq!(reread.height, 200.0);
        assert_eq!(reread.layers.len(), 2);

        // Session fields the loader restores.
        assert_eq!(reread.active_layer_id, original.active_layer_id);
        assert_eq!(reread.viewport, original.viewport);
        assert_eq!(reread.viewport.pan_x, 40.0);
        assert_eq!(reread.viewport.zoom, 2.6840000406901043);
        assert!(reread.dirty);

        // Selection.
        assert_eq!(reread.selection, original.selection);
        let sel = reread.selection.as_ref().expect("selection survives");
        assert_eq!(sel.shape, Some(SelectionShape::Rect));
        assert_eq!(sel.inverted, None);
    }

    #[test]
    fn real_dumped_model_write_is_idempotent_byte_for_byte() {
        let first = parse_fixture().to_json().expect("serialize");
        let second = PtzDocument::from_json(&first)
            .expect("re-parse")
            .to_json()
            .expect("re-serialize");
        assert_eq!(
            first, second,
            "writing an already-written document must be byte-identical"
        );
    }

    #[test]
    fn written_payload_matches_the_input_for_every_persisted_key() {
        let written = parse_fixture().to_json().expect("serialize");
        let before: serde_json::Value = serde_json::from_str(REAL_DUMPED_MODEL).unwrap();
        let after: serde_json::Value = serde_json::from_str(&written).unwrap();

        // Key SETS first: this is what catches a dropped or added field.
        // A per-key equality loop cannot, because `after["missing"]` yields
        // Null and compares equal to a real null.
        assert_exact_keys(&after, DOC_KEYS, "document");

        for key in [
            "id",
            "name",
            "width",
            "height",
            "activeLayerId",
            "selection",
            "viewport",
            "dirty",
            "format",
            "version",
        ] {
            assert!(
                same_json_value(&after[key], &before[key]),
                "key {} changed: {:?} -> {:?}",
                key,
                before[key],
                after[key]
            );
        }

        let before_layers = before["layers"].as_array().unwrap();
        let after_layers = after["layers"].as_array().unwrap();
        assert_eq!(after_layers.len(), before_layers.len());
        for (b, a) in before_layers.iter().zip(after_layers.iter()) {
            assert_exact_keys(a, LAYER_KEYS, "layer");
            for key in [
                "id",
                "name",
                "type",
                "visible",
                "opacity",
                "locked",
                "lockTransparency",
                "isBackground",
                "lockPosition",
                "lockRotation",
                "hasAdjustments",
                "blendMode",
                "transform",
                "width",
                "height",
                "imageBitmap",
                "baseImageBitmap",
            ] {
                assert!(
                    same_json_value(&a[key], &b[key]),
                    "layer key {} changed: {:?} -> {:?}",
                    key,
                    b[key],
                    a[key]
                );
            }
        }
    }

    /// The raster-only real dump cannot prove the NESTED payloads survive: it
    /// contains none of them, so a writer that dropped `textData` entirely
    /// passed every other test. This fixture carries all four content kinds and
    /// every nested value is asserted literally.
    #[test]
    fn nested_layer_payloads_survive_the_round_trip() {
        let reread = round_trip(MIXED_LAYER_TYPES_MODEL);
        assert_eq!(reread.layers.len(), 4, "all four layer kinds survive");

        // ── basicAdjustment on the raster + adjustment layers ──
        let adjusted = layer_by_name(&reread, "Adjusted");
        let adj = adjusted.basic_adjustment.as_ref().expect("basicAdjustment");
        assert_eq!(adj.brightness, 12.5);
        assert_eq!(adj.contrast, -33.25);
        assert_eq!(adj.saturation, 44.75);
        assert_eq!(
            adjusted.blend_mode,
            crate::canonical_model::BlendMode::ColorDodge
        );
        assert_eq!(adjusted.is_background, Some(true));
        assert_eq!(adjusted.lock_transparency, Some(true));
        assert_eq!(adjusted.lock_position, Some(true));
        assert_eq!(adjusted.lock_rotation, Some(true));
        assert!(adjusted.transform.flip_h && adjusted.transform.flip_v);
        assert_eq!(adjusted.transform.scale_x, 3.5);
        assert_eq!(adjusted.transform.rotation, 5.5);

        let grade = layer_by_name(&reread, "Grade");
        let gadj = grade.basic_adjustment.as_ref().expect("basicAdjustment");
        assert_eq!(gadj.brightness, -1.5);
        assert_eq!(gadj.contrast, 2.5);
        assert_eq!(gadj.saturation, -3.5);
        assert_eq!(
            grade.layer_type,
            crate::canonical_model::LayerType::Adjustment
        );

        // ── shapeParams, including nested fill and stroke ──
        let star = layer_by_name(&reread, "Star");
        let sp = star.shape_params.as_ref().expect("shapeParams");
        assert_eq!(sp.kind, crate::canonical_model::ShapeKind::Star);
        assert_eq!(sp.width, 200.0);
        assert_eq!(sp.height, 150.0);
        assert_eq!(sp.radius, 11.5);
        assert_eq!(sp.fill.kind, crate::canonical_model::ShapeFillKind::Solid);
        assert_eq!(sp.fill.color, "#E15A17");
        assert!(sp.stroke.enabled);
        assert_eq!(sp.stroke.color, "#00FF00");
        assert_eq!(sp.stroke.width, 3.5);
        assert!(!sp.arrow_head);
        assert!(!star.visible);
        assert_eq!(star.opacity, 0.25);
        assert!(star.locked);
        assert_eq!(
            star.blend_mode,
            crate::canonical_model::BlendMode::SoftLight
        );
        assert_eq!(star.transform.x, -7.25);
        assert_eq!(star.transform.scale_y, 0.75);
        assert!(star.text_data.is_none(), "shape layer has no textData");

        // ── textData, all 16 fields plus nested stroke.align ──
        let title = layer_by_name(&reread, "Title");
        let td = title.text_data.as_ref().expect("textData");
        assert_eq!(td.content, "Hello");
        assert_eq!(td.font_family, "Inter");
        assert_eq!(td.font_size, 72.0);
        assert_eq!(td.font_weight, 700.0);
        assert_eq!(td.font_style, crate::canonical_model::TextFontStyle::Italic);
        assert_eq!(td.color, "#123456");
        assert_eq!(td.align, crate::canonical_model::TextAlign::Center);
        assert_eq!(td.line_height, 2.5);
        assert_eq!(td.letter_spacing, 3.5);
        assert_eq!(td.box_mode, crate::canonical_model::TextBoxMode::Area);
        assert_eq!(td.box_width, 320.0);
        assert_eq!(td.box_height, 44.0);
        assert_eq!(td.stroke.width, 2.5);
        assert_eq!(td.stroke.color, "#ABCDEF");
        assert_eq!(
            td.stroke.align,
            Some(crate::canonical_model::TextStrokeAlign::Inside),
            "nested stroke.align survives"
        );
        assert_eq!(td.underline, Some(true));
        assert_eq!(td.strikethrough, Some(false));
        assert_eq!(td.uppercase, Some(true));
        assert_eq!(
            title.blend_mode,
            crate::canonical_model::BlendMode::HardLight
        );
        assert!(
            title.shape_params.is_none(),
            "text layer has no shapeParams"
        );

        // ── The per-layer boolean flags, asserted on EVERY layer ──
        // All four fixture layers carry these. Asserting them only on the
        // raster layer (as an earlier version of this test did) let a writer
        // that emitted `false -> null` on the other three pass everything.
        let expected_flags: [(&str, Option<bool>, Option<bool>, Option<bool>, Option<bool>); 4] = [
            ("Adjusted", Some(true), Some(true), Some(true), Some(true)),
            ("Star", Some(false), Some(false), Some(false), Some(false)),
            ("Title", Some(false), Some(false), Some(false), Some(false)),
            ("Grade", Some(false), Some(false), Some(false), Some(false)),
        ];
        for (name, is_bg, lock_t, lock_p, lock_r) in expected_flags {
            let layer = layer_by_name(&reread, name);
            assert_eq!(
                (
                    layer.is_background,
                    layer.lock_transparency,
                    layer.lock_position,
                    layer.lock_rotation
                ),
                (is_bg, lock_t, lock_p, lock_r),
                "per-layer flags changed on layer {}",
                name
            );
            assert_eq!(
                layer.has_adjustments,
                if name == "Adjusted" || name == "Grade" {
                    Some(true)
                } else {
                    Some(false)
                },
                "hasAdjustments changed on layer {}",
                name
            );
        }
    }

    /// The nested payloads are present in the WRITTEN bytes, with the exact key
    /// sets the file contract declares. This is the assertion a raster-only
    /// fixture structurally cannot make.
    #[test]
    fn nested_payload_key_sets_are_exact_in_the_written_bytes() {
        let written = PtzDocument::from_json(MIXED_LAYER_TYPES_MODEL)
            .expect("parse")
            .to_json()
            .expect("serialize");
        let v: serde_json::Value = serde_json::from_str(&written).unwrap();

        let layers = v["layers"].as_array().expect("layers array");
        // Resolved BY NAME, not by index, so reordering the fixture cannot
        // silently point an assertion at a different layer.
        let by_name = |name: &str| -> &serde_json::Value {
            layers
                .iter()
                .find(|l| l["name"] == serde_json::json!(name))
                .unwrap_or_else(|| panic!("layer {} in written bytes", name))
        };

        assert_exact_keys(
            &by_name("Adjusted")["basicAdjustment"],
            &["brightness", "contrast", "saturation"],
            "Adjusted.basicAdjustment",
        );
        assert_exact_keys(
            &by_name("Star")["shapeParams"],
            SHAPE_PARAM_KEYS,
            "Star.shapeParams",
        );
        assert_exact_keys(
            &by_name("Star")["shapeParams"]["fill"],
            &["color", "kind"],
            "Star.shapeParams.fill",
        );
        assert_exact_keys(
            &by_name("Star")["shapeParams"]["stroke"],
            &["color", "enabled", "width"],
            "Star.shapeParams.stroke",
        );
        assert_exact_keys(
            &by_name("Title")["textData"],
            TEXT_DATA_KEYS,
            "Title.textData",
        );
        assert_exact_keys(
            &by_name("Title")["textData"]["stroke"],
            &["align", "color", "width"],
            "Title.textData.stroke",
        );
        // The FOURTH layer carries a payload too, and was previously unchecked.
        assert_exact_keys(
            &by_name("Grade")["basicAdjustment"],
            &["brightness", "contrast", "saturation"],
            "Grade.basicAdjustment",
        );
        // A layer type with no payload carries an explicit null, not an absent key.
        assert_eq!(by_name("Star")["textData"], serde_json::Value::Null);
        assert_eq!(by_name("Title")["shapeParams"], serde_json::Value::Null);
        assert_eq!(by_name("Adjusted")["shapeParams"], serde_json::Value::Null);
        assert_eq!(by_name("Adjusted")["textData"], serde_json::Value::Null);
    }

    #[test]
    fn every_layer_field_survives_the_round_trip() {
        let reread = round_trip(REAL_DUMPED_MODEL);

        let painted = layer_by_name(&reread, "Painted");
        assert_eq!(painted.id, "layer-ryyklho2");
        assert_eq!(
            painted.layer_type,
            crate::canonical_model::LayerType::Raster
        );
        assert_eq!(
            painted.blend_mode,
            crate::canonical_model::BlendMode::Multiply
        );
        assert!(painted.visible);
        assert_eq!(painted.opacity, 0.75);
        assert!(painted.locked);
        assert_eq!(painted.lock_transparency, Some(true));
        assert_eq!(painted.has_adjustments, Some(false));
        assert_eq!(painted.transform.x, 0.0);
        assert_eq!(painted.transform.scale_x, 1.0);
        assert!(!painted.transform.flip_h);
        assert_eq!(painted.width, 300.0);
        assert_eq!(painted.height, 200.0);

        let background = layer_by_name(&reread, "Background");
        assert_eq!(background.id, "layer-jhgyjahw");
        assert_eq!(background.is_background, Some(true));
        assert_eq!(background.lock_position, Some(true));
        assert_eq!(background.lock_rotation, Some(true));
        assert_eq!(
            background.blend_mode,
            crate::canonical_model::BlendMode::Normal
        );
        assert_eq!(background.opacity, 1.0);
    }

    // ── Bitmap keys: the pixels-never-in-JSON contract ──

    #[test]
    fn bitmap_keys_are_always_null_even_when_the_host_sends_live_values() {
        let json = r#"{"id":"d","name":"n","width":10,"height":10,
            "layers":[{"id":"l","name":"L","type":"raster","visible":true,"opacity":1,
            "locked":false,"blendMode":"normal",
            "transform":{"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0,"flipH":false,"flipV":false},
            "width":10,"height":10,"imageBitmap":{"width":10,"height":10},
            "baseImageBitmap":{"width":10,"height":10}}]}"#;
        let written = PtzDocument::from_json(json)
            .expect("parse")
            .to_json()
            .expect("serialize");
        let v: serde_json::Value = serde_json::from_str(&written).unwrap();
        assert_eq!(v["layers"][0]["imageBitmap"], serde_json::Value::Null);
        assert_eq!(v["layers"][0]["baseImageBitmap"], serde_json::Value::Null);
    }

    // ── Backward compatibility: older files still load ──

    /// A text layer written before most `TextData` fields existed must still
    /// SAVE, not just load. `normalizeTextData` never runs on load
    /// (`editorOpenImage.ts` -> `restoreSnapshot` raw-spreads `textData`), so
    /// without serde defaults this file would load, render, and then fail its
    /// next save with `E_VALIDATION` — writing nothing.
    #[test]
    fn a_text_layer_missing_late_text_data_fields_still_saves() {
        let doc = PtzDocument::from_json(LEGACY_TEXT_LAYER_MODEL)
            .expect("a pre-TextData-fields file must still parse and save");

        let written = doc.to_json().expect("and must still serialize");

        let td = doc.layers[0].text_data.as_ref().expect("textData present");
        // The fields the file DID carry are untouched.
        assert_eq!(td.content, "legacy");
        assert_eq!(td.font_family, "Arial");
        assert_eq!(td.font_size, 36.0);
        // The absent ones take DEFAULT_TEXT_DATA values, not a parse error.
        assert_eq!(td.letter_spacing, 0.0);
        assert_eq!(td.box_mode, crate::canonical_model::TextBoxMode::Point);
        assert_eq!(td.box_width, 0.0);
        assert_eq!(td.box_height, 0.0);
        assert_eq!(td.stroke.width, 0.0);
        assert_eq!(td.stroke.color, "#000000");
        assert_eq!(td.underline, Some(false));
        assert_eq!(td.strikethrough, Some(false));
        assert_eq!(td.uppercase, Some(false));

        // And the saved bytes carry the FULL key set, so the next save of this
        // file is not lossy.
        let v: serde_json::Value = serde_json::from_str(&written).unwrap();
        assert_exact_keys(&v["layers"][0]["textData"], TEXT_DATA_KEYS, "textData");
    }

    /// The same tolerance for a shape layer whose `shapeParams` predates a
    /// field, and for a partial `basicAdjustment`.
    #[test]
    fn partial_shape_params_and_adjustments_still_save() {
        let json = r#"{"id":"d","name":"n","width":10,"height":10,"activeLayerId":null,
            "selection":null,"viewport":{"panX":0,"panY":0,"zoom":1,"rotation":0},"dirty":false,
            "layers":[
              {"id":"s","name":"S","type":"shape","visible":true,"opacity":1,"locked":false,
               "blendMode":"normal","transform":{"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0,
               "flipH":false,"flipV":false},"width":10,"height":10,
               "shapeParams":{"kind":"rect"}},
              {"id":"a","name":"A","type":"raster","visible":true,"opacity":1,"locked":false,
               "blendMode":"normal","transform":{"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0,
               "flipH":false,"flipV":false},"width":10,"height":10,
               "basicAdjustment":{"brightness":5.0}}]}"#;
        let doc = PtzDocument::from_json(json).expect("partial nested params must still parse");
        let written = doc.to_json().expect("and serialize");

        let sp = doc.layers[0].shape_params.as_ref().expect("shapeParams");
        assert_eq!(sp.kind, crate::canonical_model::ShapeKind::Rect);
        assert_eq!(sp.width, 0.0, "absent width defaults");
        assert_eq!(sp.fill.kind, crate::canonical_model::ShapeFillKind::None);
        assert!(!sp.stroke.enabled);

        let adj = doc.layers[1]
            .basic_adjustment
            .as_ref()
            .expect("basicAdjustment");
        assert_eq!(adj.brightness, 5.0);
        assert_eq!(adj.contrast, 0.0, "absent contrast defaults to neutral");

        let v: serde_json::Value = serde_json::from_str(&written).unwrap();
        assert_exact_keys(
            &v["layers"][0]["shapeParams"],
            SHAPE_PARAM_KEYS,
            "shapeParams",
        );
        assert_exact_keys(
            &v["layers"][1]["basicAdjustment"],
            &["brightness", "contrast", "saturation"],
            "basicAdjustment",
        );
    }

    /// KNOWN, TESTED on-disk differences from the previous TypeScript writer.
    /// Not a hazard — the loader does `JSON.parse` + `restoreSnapshot`, both of
    /// which ignore unknown keys and treat `null` as absent — but recorded so a
    /// future reader is not surprised.
    #[test]
    fn the_on_disk_delta_from_the_previous_writer_is_as_documented() {
        let written = parse_fixture().to_json().expect("serialize");
        let v: serde_json::Value = serde_json::from_str(&written).unwrap();
        let layer = &v["layers"][0];

        // DELTA 1: `resourceId` is now emitted as an explicit null on every
        // layer. The previous writer had no such key. Unknown keys are ignored
        // by the loader, so this is inert.
        assert_eq!(layer["resourceId"], serde_json::Value::Null);
        assert!(
            layer.as_object().unwrap().contains_key("resourceId"),
            "resourceId is always present"
        );

        // DELTA 2: an absent optional is written as an explicit null rather
        // than omitted (`selection` was `null` in the fixture; `basicAdjustment`
        // was absent on the Painted layer and is now an explicit null).
        assert_eq!(layer["basicAdjustment"], serde_json::Value::Null);
        assert_eq!(
            v["activeLayerId"],
            serde_json::Value::String("layer-ryyklho2".into())
        );

        // DELTA 3: `bitmapEpoch` is no longer written. It is transient and
        // `restoreSnapshot` clears it on load, so nothing reads it back.
        assert!(
            !layer.as_object().unwrap().contains_key("bitmapEpoch"),
            "bitmapEpoch is transient and must not be persisted"
        );

        // DELTA 4: floats are now parsed CORRECTLY-ROUNDED, so the writer echoes the
        // file's own decimal literals back unchanged. `serde_json`'s default
        // float parsing is approximate and can be 1 ULP off on some literals;
        // `crates/core/Cargo.toml` enables its `float_roundtrip` feature so
        // Rust parses exactly as `str::parse::<f64>` and as the TypeScript
        // loader's `JSON.parse` do. Measured on this fixture: `panY` was
        // `63.599995930989564` (0x404fccccaaaaaaa8) and is written back as
        // exactly that, where the default path would have emitted
        // `63.59999593098957` (0x404fccccaaaaaaa9).
        //
        // This means every float in every `.ptz` this build writes can differ
        // from what the previous TypeScript writer emitted -- by at most 1 ULP
        // on the literals where the approximate path was inexact, and not at
        // all elsewhere. Recorded here because it is a real on-disk change, not
        // an internal detail.
        assert!(
            written.contains("\"panY\":63.599995930989564"),
            "floats must be echoed back exactly; got: {}",
            &written[written.len().saturating_sub(60)..]
        );

        // DELTA 5: keys are emitted ALPHABETICALLY, not in struct declaration
        // order. `to_json` routes through `serde_json::to_value`, and
        // serde_json's `Map` is a `BTreeMap` because the `preserve_order`
        // feature is not enabled (`crates/core/Cargo.toml`). JSON objects are
        // unordered and the loader reads by key, so this is inert — asserted on
        // the RAW BYTES so a byte-diff cannot read as a regression.
        //
        // Declaration order would put `dirty` (declared 7th) AFTER `version`
        // (declared last); alphabetical puts `dirty` before `format`.
        let raw = written.as_str();
        let dirty_at = raw.find("\"dirty\"").expect("dirty key present");
        let format_at = raw.find("\"format\"").expect("format key present");
        let version_at = raw.find("\"version\"").expect("version key present");
        assert!(
            dirty_at < format_at && format_at < version_at,
            "document keys must be emitted alphabetically: dirty < format < version \
             (got dirty@{}, format@{}, version@{})",
            dirty_at,
            format_at,
            version_at
        );
        // And the document opens with its first key alphabetically, proving the
        // whole object is sorted rather than just those three.
        assert!(
            raw.starts_with("{\"activeLayerId\":"),
            "document must start with its alphabetically-first key, got: {}",
            &raw[..raw.len().min(40)]
        );

        // DELTA 6: a float-valued field is written with an explicit decimal
        // point (`300.0`) where the fixture had the integer `300`. Same number,
        // different spelling — only visible in the raw bytes, since
        // `as_f64()` accepts both.
        assert!(
            raw.contains("\"height\":200.0,"),
            "floats must be written with an explicit .0; got: {}",
            &raw[..raw.len().min(80)]
        );
        // `width` is alphabetically last, so it carries no trailing comma.
        assert!(
            raw.ends_with("\"width\":300.0}"),
            "the alphabetically-last key ends the document; got: ..{}",
            &raw[raw.len().saturating_sub(20)..]
        );
        assert_eq!(v["width"].as_f64(), Some(300.0));
    }

    #[test]
    fn v1_file_without_version_or_viewport_loads_with_usable_defaults() {
        let json = r#"{"id":"d","name":"n","width":10,"height":10,"activeLayerId":null,
            "layers":[{"id":"l","name":"L","type":"raster","visible":true,"opacity":1,
            "locked":false,"blendMode":"normal",
            "transform":{"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0,"flipH":false,"flipV":false},
            "width":10,"height":10}]}"#;
        let doc = PtzDocument::from_json(json).expect("pre-version file must parse");
        assert_eq!(doc.id, "d");
        assert_eq!(
            doc.viewport.zoom, 1.0,
            "missing viewport defaults to 100% zoom"
        );
        assert!(!doc.dirty);
    }

    #[test]
    fn unknown_fields_from_a_newer_build_are_ignored() {
        let json = r#"{"id":"d","name":"n","width":10,"height":10,"futureField":42,
            "layers":[{"id":"l","name":"L","type":"raster","visible":true,"opacity":1,
            "locked":false,"blendMode":"normal",
            "transform":{"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0,"flipH":false,"flipV":false},
            "width":10,"height":10,"futureLayerField":true}]}"#;
        let doc = PtzDocument::from_json(json).expect("unknown keys are ignored");
        assert_eq!(doc.layers.len(), 1);
    }

    // ── Fail loudly: the writer must never emit a document it cannot read ──

    #[test]
    fn rejects_malformed_json_instead_of_writing_garbage() {
        assert!(PtzDocument::from_json("{ not json").is_err());
    }

    #[test]
    fn rejects_an_out_of_contract_field_type() {
        // `opacity` is a number on the wire; a string must fail the save.
        let json = r#"{"id":"d","name":"n","width":10,"height":10,"layers":[{"id":"l",
            "name":"L","type":"raster","visible":true,"opacity":"opaque","locked":false,
            "blendMode":"normal","transform":{"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0,
            "flipH":false,"flipV":false},"width":10,"height":10}]}"#;
        assert!(
            PtzDocument::from_json(json).is_err(),
            "a wrong-typed field must fail the save, not round-trip as garbage"
        );
    }

    #[test]
    fn rejects_an_unknown_blend_mode_rather_than_downgrading_it() {
        let json = r#"{"id":"d","name":"n","width":10,"height":10,"layers":[{"id":"l",
            "name":"L","type":"raster","visible":true,"opacity":1,"locked":false,
            "blendMode":"not-a-mode","transform":{"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0,
            "flipH":false,"flipV":false},"width":10,"height":10}]}"#;
        assert!(PtzDocument::from_json(json).is_err());
    }

    #[test]
    fn rejects_a_missing_required_document_field() {
        let json = r#"{"name":"n","width":10,"height":10,"layers":[]}"#;
        assert!(PtzDocument::from_json(json).is_err(), "id is required");
    }

    // ── Header contract ──

    #[test]
    fn writes_the_v3_header_the_loader_expects() {
        let json = parse_fixture().to_json().expect("serialize");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            v["format"],
            serde_json::Value::String("photrez-ptz".to_string())
        );
        assert_eq!(v["version"], serde_json::Value::Number(3.into()));
        // The loader warns on `version > 3`, so a raise must not land here
        // without teaching editorOpenImage.ts the new version first.
        assert!(v["version"].as_u64().unwrap() <= 3);
    }

    #[test]
    fn to_canonical_drops_session_fields() {
        let v = serde_json::to_value(&parse_fixture().to_canonical()).unwrap();
        assert!(v.get("activeLayerId").is_none());
        assert!(v.get("viewport").is_none());
        assert!(v.get("dirty").is_none());
        assert_eq!(v["layers"].as_array().map(|l| l.len()), Some(2));
    }
}
