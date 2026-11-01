// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::ptz::content::*;
use serde::Serialize;
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

fn raster_layer() -> PtzLayer {
    PtzLayer {
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

fn shape_layer() -> PtzLayer {
    PtzLayer {
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

fn text_layer() -> PtzLayer {
    PtzLayer {
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
    let doc = PtzDocumentContent {
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
    let back: PtzDocumentContent = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(doc, back, "round-trip must be value-identical");

    // Field-name compatibility check: the payload is camelCase, as saved files are.
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

/// Completeness guard: every persisted layer field
/// has a counterpart in PtzLayer. We fully populate one layer and
/// assert the exact serialized key set, so no field is silently dropped.
#[test]
fn canonical_layer_is_superset_of_ts_layernode() {
    let layer = PtzLayer {
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
    let got: std::collections::BTreeSet<String> = v.as_object().unwrap().keys().cloned().collect();

    // Expected keys = every persisted layer field except the
    // excluded bitmapEpoch and the two bitmap placeholder keys (those collapse into
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
        "PtzLayer key set must equal LayerNode superset"
    );
}
