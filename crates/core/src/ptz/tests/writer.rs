// SPDX-License-Identifier: AGPL-3.0-or-later
//! Write-side tests for `to_ptz_document_json` (v4 `.ptz` `document.json`).

use crate::ptz::content::*;

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

fn raster() -> PtzLayer {
    PtzLayer {
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

fn shape() -> PtzLayer {
    PtzLayer {
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

fn text() -> PtzLayer {
    PtzLayer {
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

fn doc3() -> PtzDocumentContent {
    PtzDocumentContent {
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
    let parsed = PtzDocumentContent::from_ptz_document_json(&json).expect("v4 parse");
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
    let parsed = PtzDocumentContent::from_ptz_document_json(&json).expect("v4 parse");
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
    let parsed = PtzDocumentContent::from_ptz_document_json(&json);
    assert!(
        parsed.is_ok(),
        "v4 output must be readable by from_ptz_document_json"
    );
}
