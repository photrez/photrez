// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::fixtures::REAL_DUMPED_MODEL;
use photrez_document::content::*;

#[test]
fn parses_real_dumped_model() {
    let doc = PtzDocumentContent::from_ptz_document_json(REAL_DUMPED_MODEL)
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
    let doc = PtzDocumentContent::from_ptz_document_json(json).expect("minimal doc must parse");
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
    let res = PtzDocumentContent::from_ptz_document_json("{ not json");
    assert!(res.is_err(), "malformed JSON must return Err, not panic");
}

#[test]
fn ignores_unknown_fields() {
    let json = r#"{"id":"d","name":"n","width":10,"height":10,"bogusTop":123,"layers":[{"id":"l","name":"L","type":"raster","visible":true,"opacity":1,"locked":false,"blendMode":"normal","transform":{"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0,"flipH":false,"flipV":false},"width":10,"height":10,"bogusLayer":"x"}]}"#;
    let doc = PtzDocumentContent::from_ptz_document_json(json)
        .expect("unknown top-level and layer keys must be ignored");
    assert_eq!(doc.layers.len(), 1);
}
