// SPDX-License-Identifier: AGPL-3.0-or-later
// Projection tests: the persisted-layer to `LayerMeta` value bridge.
use crate::document::content::*;
use crate::document::layer::{LayerMeta, Rect};
use crate::engine::projection::*;

// Duplicated from the ptz content test module (crates/document/tests/content.rs): the bridge tests
// below call raster_layer()/shape_layer(), which live there. Kept tiny and
// identical to avoid cross-module test-visibility coupling.
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

fn fully_populated_layer() -> PtzLayer {
    PtzLayer {
        id: "canon-id".to_string(),
        name: "canon-name".to_string(),
        layer_type: LayerType::Text,
        visible: true,
        opacity: 0.55,
        locked: true,
        is_background: Some(true),
        lock_transparency: Some(true),
        lock_position: Some(true),
        lock_rotation: Some(true),
        has_adjustments: Some(true),
        basic_adjustment: Some(BasicAdjustment {
            brightness: 1.0,
            contrast: 2.0,
            saturation: 3.0,
        }),
        resource_id: Some(9),
        blend_mode: BlendMode::Overlay,
        transform: Transform2D {
            x: 11.0,
            y: 22.0,
            scale_x: 1.5,
            scale_y: 2.5,
            rotation: 33.0,
            flip_h: true,
            flip_v: false,
        },
        width: 700.0,
        height: 500.0,
        shape_params: Some(ShapeParams {
            kind: ShapeKind::Heart,
            width: 90.0,
            height: 80.0,
            radius: 5.0,
            fill: ShapeFill {
                kind: ShapeFillKind::Solid,
                color: "#ABCDEF".to_string(),
            },
            stroke: ShapeStroke {
                enabled: false,
                color: "#000000".to_string(),
                width: 0.0,
            },
            arrow_head: true,
        }),
        text_data: Some(TextData {
            content: "Z".to_string(),
            font_family: "Tahoma".to_string(),
            font_size: 24.0,
            font_weight: 300.0,
            font_style: TextFontStyle::Normal,
            color: "#123456".to_string(),
            align: TextAlign::Right,
            line_height: 1.0,
            letter_spacing: 2.0,
            box_mode: TextBoxMode::Point,
            box_width: 10.0,
            box_height: 20.0,
            stroke: TextStroke {
                width: 1.0,
                color: "#654321".to_string(),
                align: Some(TextStrokeAlign::Inside),
            },
            underline: Some(true),
            strikethrough: Some(false),
            uppercase: Some(false),
        }),
    }
}

fn distinct_render_layer() -> LayerMeta {
    LayerMeta {
        id: "render-id".to_string(),
        name: "render-name".to_string(),
        visible: false,
        opacity: 0.12,
        resource_id: 99,
        x: 100.0,
        y: 200.0,
        scale_x: 3.0,
        scale_y: 4.0,
        rotation: 45.0,
        dirty_rect: Some(Rect {
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        }),
        ..Default::default()
    }
}

#[test]
fn render_layer_mapping_uses_distinct_overlap_values() {
    // Every overlap field gets a unique, recognizable value so a swap would
    // be caught by the assertions below.
    let c = PtzLayer {
        id: "c-id".to_string(),
        name: "c-name".to_string(),
        layer_type: LayerType::Shape,
        visible: false,
        opacity: 0.42,
        locked: true,
        is_background: Some(false),
        lock_transparency: Some(true),
        lock_position: None,
        lock_rotation: None,
        has_adjustments: Some(false),
        basic_adjustment: None,
        resource_id: Some(7),
        blend_mode: BlendMode::Screen,
        transform: Transform2D {
            x: 1.0,
            y: 2.0,
            scale_x: 3.0,
            scale_y: 4.0,
            rotation: 5.0,
            flip_h: true,
            flip_v: false,
        },
        width: 100.0,
        height: 120.0,
        shape_params: Some(ShapeParams {
            kind: ShapeKind::Star,
            width: 50.0,
            height: 60.0,
            radius: 7.0,
            fill: ShapeFill {
                kind: ShapeFillKind::Solid,
                color: "#FFFFFF".to_string(),
            },
            stroke: ShapeStroke {
                enabled: true,
                color: "#000000".to_string(),
                width: 1.0,
            },
            arrow_head: true,
        }),
        text_data: None,
    };

    let r = to_render_layer(&c);
    assert_eq!(r.id, "c-id");
    assert_eq!(r.name, "c-name");
    assert!(!r.visible);
    assert_eq!(r.opacity, 0.42);
    assert_eq!(r.resource_id, 7);
    assert_eq!(r.x, 1.0);
    assert_eq!(r.y, 2.0);
    assert_eq!(r.scale_x, 3.0);
    assert_eq!(r.scale_y, 4.0);
    assert_eq!(r.rotation, 5.0);
    assert_eq!(
        r.dirty_rect, None,
        "render layer bridge never sets dirty_rect"
    );
}

#[test]
fn merge_round_trip_preserves_canonical_with_resource() {
    let mut base = raster_layer();
    base.resource_id = Some(42);
    let r = to_render_layer(&base);
    assert_eq!(r.resource_id, 42);
    let merged = merge_into_layer(&base, &r);
    assert_eq!(merged, base, "round-trip must be value-identical");
}

#[test]
fn merge_round_trip_preserves_canonical_without_resource() {
    let base = shape_layer();
    assert_eq!(base.resource_id, None);
    let r = to_render_layer(&base);
    assert_eq!(r.resource_id, 0, "absent resource maps to 0");
    let merged = merge_into_layer(&base, &r);
    assert_eq!(merged, base, "None -> 0 -> None must round-trip");
}

#[test]
fn merge_preserves_all_canonical_only_fields() {
    let c = fully_populated_layer();
    let r = distinct_render_layer();
    let m = merge_into_layer(&c, &r);

    // Overlap fields come from the render layer.
    assert_eq!(m.id, "render-id");
    assert_eq!(m.name, "render-name");
    assert!(!m.visible);
    assert_eq!(m.opacity, 0.12);
    assert_eq!(m.resource_id, Some(99));
    assert_eq!(m.transform.x, 100.0);
    assert_eq!(m.transform.y, 200.0);
    assert_eq!(m.transform.scale_x, 3.0);
    assert_eq!(m.transform.scale_y, 4.0);
    assert_eq!(m.transform.rotation, 45.0);

    // The 14 canonical-only fields survive untouched (no silent loss).
    assert_eq!(m.layer_type, c.layer_type);
    assert_eq!(m.locked, c.locked);
    assert_eq!(m.is_background, c.is_background);
    assert_eq!(m.lock_transparency, c.lock_transparency);
    assert_eq!(m.lock_position, c.lock_position);
    assert_eq!(m.lock_rotation, c.lock_rotation);
    assert_eq!(m.has_adjustments, c.has_adjustments);
    assert_eq!(m.basic_adjustment, c.basic_adjustment);
    assert_eq!(m.blend_mode, c.blend_mode);
    assert_eq!(m.transform.flip_h, c.transform.flip_h);
    assert_eq!(m.transform.flip_v, c.transform.flip_v);
    assert_eq!(m.width, c.width);
    assert_eq!(m.height, c.height);
    assert_eq!(m.shape_params, c.shape_params);
    assert_eq!(m.text_data, c.text_data);
}

#[test]
fn merge_takes_render_layer_nested_params_when_present() {
    // D-b ratchet: an extended Some on the render layer (set by the typed
    // AddLayer / SetLayerParams / SetAdjustment arms) overrides the canonical
    // base; a None on the render layer preserves the base value.
    let base = shape_layer();
    let mut r = to_render_layer(&base);
    let new_params = ShapeParams {
        kind: ShapeKind::Heart,
        width: 90.0,
        height: 80.0,
        radius: 5.0,
        fill: ShapeFill {
            kind: ShapeFillKind::Solid,
            color: "#ABCDEF".to_string(),
        },
        stroke: ShapeStroke {
            enabled: false,
            color: "#000000".to_string(),
            width: 0.0,
        },
        arrow_head: true,
    };
    r.shape_params = Some(new_params.clone());
    // text_data / basic_adjustment left None on the render layer -> base preserved.
    let m = merge_into_layer(&base, &r);
    assert_eq!(m.shape_params, Some(new_params), "render Some must take");
    assert_eq!(m.text_data, base.text_data, "render None preserves base");
    assert_eq!(
        m.basic_adjustment, base.basic_adjustment,
        "render None preserves base"
    );
}

#[test]
fn merge_takes_render_layer_text_data_and_basic_adjustment_when_present() {
    // Some-takes assertions for text_data AND basic_adjustment
    // (the sibling test above only covers shape_params). A Some on the render
    // layer overrides the canonical base; a None on the render layer preserves it.
    let base = shape_layer();
    let mut r = to_render_layer(&base);
    let td = TextData {
        content: "sel-fix".to_string(),
        font_family: "Arial".to_string(),
        font_size: 32.0,
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
        underline: Some(false),
        strikethrough: Some(false),
        uppercase: Some(false),
    };
    let ba = BasicAdjustment {
        brightness: 1.0,
        contrast: 2.0,
        saturation: 3.0,
    };
    r.text_data = Some(td.clone());
    r.basic_adjustment = Some(ba.clone());
    // shape_params left None -> base preserved.
    let m = merge_into_layer(&base, &r);
    assert_eq!(m.text_data, Some(td), "render text_data Some must take");
    assert_eq!(
        m.basic_adjustment,
        Some(ba),
        "render basic_adjustment Some must take"
    );
    assert_eq!(
        m.shape_params, base.shape_params,
        "render None preserves base"
    );
}

#[test]
fn dirty_rect_does_not_leak_into_canonical() {
    let base = raster_layer();
    let r_with = LayerMeta {
        id: "r1".to_string(),
        name: "n1".to_string(),
        visible: true,
        opacity: 0.5,
        resource_id: 3,
        x: 5.0,
        y: 6.0,
        scale_x: 2.0,
        scale_y: 2.0,
        rotation: 9.0,
        dirty_rect: Some(Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        }),
        ..Default::default()
    };
    let r_without = LayerMeta {
        id: "r1".to_string(),
        name: "n1".to_string(),
        visible: true,
        opacity: 0.5,
        resource_id: 3,
        x: 5.0,
        y: 6.0,
        scale_x: 2.0,
        scale_y: 2.0,
        rotation: 9.0,
        dirty_rect: None,
        ..Default::default()
    };

    let m_with = merge_into_layer(&base, &r_with);
    let m_without = merge_into_layer(&base, &r_without);
    assert_eq!(
        m_with, m_without,
        "dirty_rect has no canonical counterpart and must be ignored"
    );
    // Transform reflects only the overlap fields, not the dirty rect.
    assert_eq!(m_with.transform.x, 5.0);
    assert_eq!(m_with.transform.scale_x, 2.0);
    assert_eq!(m_with.transform.rotation, 9.0);
    assert_eq!(m_with.transform.flip_h, base.transform.flip_h);
    assert_eq!(m_with.transform.flip_v, base.transform.flip_v);
}
