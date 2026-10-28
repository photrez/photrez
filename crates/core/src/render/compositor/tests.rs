use super::*;
use crate::pixel::store::PixelStoreRegistry;
use crate::ptz::content::{BlendMode, LayerType, PtzDocumentContent, PtzLayer, Transform2D};
use crate::ptz::layer_params::{
    ShapeFill, ShapeFillKind, ShapeKind, ShapeParams, ShapeStroke, TextData,
};

const GOLDEN_WIDTH: u32 = 4;
const GOLDEN_HEIGHT: u32 = 4;
// Browser Canvas2D: bottom red 4x4 opaque, top blue 2x2 at (0,0) with
// globalAlpha 0.5, source-over, read back row-major RGBA.
const CANVAS2D_GOLDEN: [u8; 64] = [
    127, 0, 128, 255, 127, 0, 128, 255, 255, 0, 0, 255, 255, 0, 0, 255, //
    127, 0, 128, 255, 127, 0, 128, 255, 255, 0, 0, 255, 255, 0, 0, 255, //
    255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, //
    255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
];

// Browser Canvas2D, 4x4: a 2x2 pattern at (1,1) with each flip variant.
const TRANSLATE_GOLDEN: [u8; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, //
    0, 0, 0, 0, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 0, 0, //
    0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 0, 255, 0, 0, 0, 0, //
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
const FLIP_H_GOLDEN: [u8; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, //
    0, 0, 0, 0, 0, 255, 0, 255, 255, 0, 0, 255, 0, 0, 0, 0, //
    0, 0, 0, 0, 255, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 0, //
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
const FLIP_V_GOLDEN: [u8; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, //
    0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 0, 255, 0, 0, 0, 0, //
    0, 0, 0, 0, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 0, 0, //
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
const FLIP_BOTH_GOLDEN: [u8; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, //
    0, 0, 0, 0, 255, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 0, //
    0, 0, 0, 0, 0, 255, 0, 255, 255, 0, 0, 255, 0, 0, 0, 0, //
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
const SCALE2_GOLDEN: [u8; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, //
    0, 0, 0, 0, 255, 0, 0, 255, 191, 64, 0, 255, 64, 191, 0, 255, //
    0, 0, 0, 0, 191, 0, 64, 255, 159, 64, 48, 255, 96, 191, 16, 255, //
    0, 0, 0, 0, 64, 0, 191, 255, 96, 64, 143, 255, 159, 191, 48, 255,
];
const ROT90_GOLDEN: [u8; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, //
    0, 0, 0, 0, 0, 0, 255, 255, 255, 0, 0, 255, 0, 0, 0, 0, //
    0, 0, 0, 0, 255, 255, 0, 255, 0, 255, 0, 255, 0, 0, 0, 0, //
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
const ROT45_GOLDEN: [u8; 64] = [
    0, 0, 0, 0, 255, 0, 0, 53, 255, 0, 0, 53, 0, 0, 0, 0, //
    0, 0, 255, 53, 127, 0, 127, 180, 127, 127, 0, 180, 0, 255, 0, 53, //
    0, 0, 255, 53, 127, 127, 127, 180, 127, 255, 0, 180, 0, 255, 0, 53, //
    0, 0, 0, 0, 255, 255, 0, 53, 255, 255, 0, 53, 0, 0, 0, 0,
];
const HALF_TRANSLATE_GOLDEN: [u8; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, //
    0, 0, 0, 0, 255, 0, 0, 128, 128, 128, 0, 128, 0, 255, 0, 128, //
    0, 0, 0, 0, 128, 0, 128, 128, 128, 128, 64, 255, 128, 255, 0, 128, //
    0, 0, 0, 0, 0, 0, 255, 128, 128, 128, 128, 128, 255, 255, 0, 128,
];
const HALF_SCALE_GOLDEN: [u8; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, //
    0, 0, 0, 0, 128, 128, 64, 255, 0, 0, 0, 0, 0, 0, 0, 0, //
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, //
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

fn raster_layer(id: &str, width: f64, height: f64, opacity: f64, visible: bool) -> PtzLayer {
    PtzLayer {
        id: id.to_owned(),
        name: id.to_owned(),
        layer_type: LayerType::Raster,
        visible,
        opacity,
        locked: false,
        is_background: None,
        lock_transparency: None,
        lock_position: None,
        lock_rotation: None,
        has_adjustments: None,
        basic_adjustment: None,
        resource_id: None,
        blend_mode: BlendMode::Normal,
        transform: Transform2D {
            x: 0.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
        },
        width,
        height,
        shape_params: None,
        text_data: None,
    }
}

fn document(layers: Vec<PtzLayer>) -> PtzDocumentContent {
    PtzDocumentContent {
        id: "composite-order".to_owned(),
        name: "Composite order".to_owned(),
        width: GOLDEN_WIDTH as f64,
        height: GOLDEN_HEIGHT as f64,
        layers,
        selection: None,
    }
}

fn seed(
    registry: &mut PixelStoreRegistry,
    doc_id: &str,
    id: &str,
    width: u32,
    height: u32,
    color: [u8; 4],
) {
    let pixels = color
        .iter()
        .copied()
        .cycle()
        .take(width as usize * height as usize * 4)
        .collect();
    registry
        .add_layer(doc_id, id, width, height, pixels)
        .expect("test layer seeds");
}

#[test]
fn two_layer_order_opacity_and_visibility_match_canvas2d() {
    let mut registry = PixelStoreRegistry::new();
    registry.open_document("composite-order");
    seed(
        &mut registry,
        "composite-order",
        "top-blue",
        2,
        2,
        [0, 0, 255, 255],
    );
    seed(
        &mut registry,
        "composite-order",
        "hidden-green",
        4,
        4,
        [0, 255, 0, 255],
    );
    seed(
        &mut registry,
        "composite-order",
        "zero-opacity",
        4,
        4,
        [0, 255, 0, 255],
    );
    seed(
        &mut registry,
        "composite-order",
        "bottom-red",
        4,
        4,
        [255, 0, 0, 255],
    );

    let content = document(vec![
        raster_layer("top-blue", 2.0, 2.0, 0.5, true),
        raster_layer("hidden-green", 4.0, 4.0, 1.0, false),
        raster_layer("zero-opacity", 4.0, 4.0, 0.0, true),
        raster_layer("bottom-red", 4.0, 4.0, 1.0, true),
    ]);

    let result = composite_layers(&content, &registry).expect("composite succeeds");
    assert_eq!((result.width, result.height), (GOLDEN_WIDTH, GOLDEN_HEIGHT));
    assert_eq!(
        result.pixels, CANVAS2D_GOLDEN,
        "CPU composite must match browser Canvas2D source-over"
    );
}

#[test]
fn all_blend_modes_match_canvas2d_goldens_within_rounding() {
    let cases = [
        (
            BlendMode::Normal,
            1.0,
            [100, 150, 200, 255],
            [150, 125, 125, 255],
        ),
        (
            BlendMode::Multiply,
            1.0,
            [78, 59, 39, 255],
            [139, 79, 45, 255],
        ),
        (
            BlendMode::Screen,
            1.0,
            [222, 191, 211, 255],
            [211, 146, 131, 255],
        ),
        (
            BlendMode::Overlay,
            1.0,
            [188, 118, 78, 255],
            [194, 109, 64, 255],
        ),
        (
            BlendMode::Darken,
            1.0,
            [100, 100, 50, 255],
            [150, 100, 50, 255],
        ),
        (
            BlendMode::Lighten,
            1.0,
            [200, 150, 200, 255],
            [200, 125, 125, 255],
        ),
        (
            BlendMode::ColorDodge,
            1.0,
            [255, 243, 232, 255],
            [228, 172, 141, 255],
        ),
        (
            BlendMode::ColorBurn,
            1.0,
            [115, 0, 0, 255],
            [157, 50, 25, 255],
        ),
        (
            BlendMode::SoftLight,
            1.0,
            [191, 111, 86, 255],
            [195, 105, 68, 255],
        ),
        (
            BlendMode::HardLight,
            1.0,
            [157, 127, 166, 255],
            [178, 114, 109, 255],
        ),
        (
            BlendMode::Difference,
            1.0,
            [100, 50, 150, 255],
            [150, 75, 100, 255],
        ),
        (
            BlendMode::Exclusion,
            1.0,
            [143, 132, 171, 255],
            [171, 116, 111, 255],
        ),
        (
            BlendMode::Normal,
            0.5,
            [100, 150, 200, 255],
            [150, 125, 125, 255],
        ),
        (
            BlendMode::Multiply,
            0.5,
            [78, 59, 39, 255],
            [139, 79, 45, 255],
        ),
        (
            BlendMode::Screen,
            0.5,
            [222, 191, 211, 255],
            [211, 146, 131, 255],
        ),
        (
            BlendMode::Overlay,
            0.5,
            [188, 118, 78, 255],
            [194, 109, 64, 255],
        ),
        (
            BlendMode::Darken,
            0.5,
            [100, 100, 50, 255],
            [150, 100, 50, 255],
        ),
        (
            BlendMode::Lighten,
            0.5,
            [200, 150, 200, 255],
            [200, 125, 125, 255],
        ),
        (
            BlendMode::ColorDodge,
            0.5,
            [255, 243, 232, 255],
            [228, 172, 141, 255],
        ),
        (
            BlendMode::ColorBurn,
            0.5,
            [115, 0, 0, 255],
            [157, 50, 25, 255],
        ),
        (
            BlendMode::SoftLight,
            0.5,
            [191, 111, 86, 255],
            [195, 105, 68, 255],
        ),
        (
            BlendMode::HardLight,
            0.5,
            [157, 127, 166, 255],
            [178, 114, 109, 255],
        ),
        (
            BlendMode::Difference,
            0.5,
            [100, 50, 150, 255],
            [150, 75, 100, 255],
        ),
        (
            BlendMode::Exclusion,
            0.5,
            [143, 132, 171, 255],
            [171, 116, 111, 255],
        ),
    ];

    for (index, (mode, opacity, _, _)) in cases.iter().enumerate() {
        let doc_id = format!("composite-blend-{index}");
        let mut registry = PixelStoreRegistry::new();
        registry.open_document(&doc_id);
        seed(&mut registry, &doc_id, "bottom", 1, 1, [200, 100, 50, 255]);
        seed(&mut registry, &doc_id, "top", 1, 1, [100, 150, 200, 255]);

        let mut top = raster_layer("top", 1.0, 1.0, *opacity, true);
        top.blend_mode = mode.clone();
        let content = PtzDocumentContent {
            id: doc_id,
            name: "Composite blend".to_owned(),
            width: 1.0,
            height: 1.0,
            layers: vec![top, raster_layer("bottom", 1.0, 1.0, 1.0, true)],
            selection: None,
        };
        let result = composite_layers(&content, &registry).expect("composite succeeds");
        let expected = if *opacity == 1.0 {
            cases[index].2
        } else {
            cases[index].3
        };
        let delta = result
            .pixels
            .iter()
            .zip(expected.iter())
            .map(|(actual, expected)| actual.abs_diff(*expected))
            .max()
            .unwrap_or(0);
        // 17 of the 24 goldens are byte-exact. Seven differ by one channel
        // unit because Canvas2D blends in premultiplied 8-bit. Never widen
        // this bound: a larger delta is a formula bug, not rounding.
        assert!(
            delta <= 1,
            "{mode:?} at opacity {opacity}: got {:?}, expected {expected:?}, max delta {delta}",
            result.pixels
        );
    }
}

#[test]
fn integer_translation_and_flips_match_canvas2d() {
    let pattern: [u8; 16] = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
    ];
    let cases = [
        (false, false, TRANSLATE_GOLDEN),
        (true, false, FLIP_H_GOLDEN),
        (false, true, FLIP_V_GOLDEN),
        (true, true, FLIP_BOTH_GOLDEN),
    ];

    for (index, (flip_h, flip_v, golden)) in cases.into_iter().enumerate() {
        let doc_id = format!("composite-transform-{index}");
        let mut registry = PixelStoreRegistry::new();
        registry.open_document(&doc_id);
        registry
            .add_layer(&doc_id, "pattern", 2, 2, pattern.to_vec())
            .expect("pattern seeds");

        let mut layer = raster_layer("pattern", 2.0, 2.0, 1.0, true);
        layer.transform.x = 1.0;
        layer.transform.y = 1.0;
        layer.transform.flip_h = flip_h;
        layer.transform.flip_v = flip_v;
        let content = PtzDocumentContent {
            id: doc_id,
            name: "Composite transform".to_owned(),
            width: 4.0,
            height: 4.0,
            layers: vec![layer],
            selection: None,
        };
        let result = composite_layers(&content, &registry).expect("composite succeeds");
        assert_eq!(result.pixels, golden, "flip_h={flip_h} flip_v={flip_v}");
    }
}

#[test]
fn transformed_layers_match_canvas2d_within_one_unit() {
    let pattern: [u8; 16] = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
    ];
    // The four axis-aligned cases are byte-exact. At 45 degrees Canvas2D's
    // color sample rounds 127.5 down once; no case may exceed one unit.
    let cases = [
        ("scale2", 1.0, 1.0, 2.0, 2.0, 0.0, SCALE2_GOLDEN, 0u8),
        ("rot90", 1.0, 1.0, 1.0, 1.0, 90.0, ROT90_GOLDEN, 0),
        ("rot45", 1.0, 1.0, 1.0, 1.0, 45.0, ROT45_GOLDEN, 1),
        (
            "halfTranslate",
            1.5,
            1.5,
            1.0,
            1.0,
            0.0,
            HALF_TRANSLATE_GOLDEN,
            0,
        ),
        ("halfScale", 1.0, 1.0, 0.5, 0.5, 0.0, HALF_SCALE_GOLDEN, 0),
    ];

    for (name, x, y, scale_x, scale_y, rotation, golden, tolerance) in cases {
        let doc_id = format!("composite-blend-{name}");
        let mut registry = PixelStoreRegistry::new();
        registry.open_document(&doc_id);
        registry
            .add_layer(&doc_id, "pattern", 2, 2, pattern.to_vec())
            .expect("pattern seeds");

        let mut layer = raster_layer("pattern", 2.0, 2.0, 1.0, true);
        layer.transform.x = x;
        layer.transform.y = y;
        layer.transform.scale_x = scale_x;
        layer.transform.scale_y = scale_y;
        layer.transform.rotation = rotation;
        let content = PtzDocumentContent {
            id: doc_id,
            name: "Composite transform".to_owned(),
            width: 4.0,
            height: 4.0,
            layers: vec![layer],
            selection: None,
        };
        let result = composite_layers(&content, &registry).expect("composite succeeds");
        for (actual, expected) in result.pixels.iter().zip(golden.iter()) {
            let delta = actual.abs_diff(*expected);
            assert!(
                delta <= tolerance,
                "{name}: got {:?}, expected {golden:?}, delta {delta}",
                result.pixels
            );
        }
    }
}

#[test]
fn shape_layer_composites_its_bitmap_at_the_transform() {
    let mut registry = PixelStoreRegistry::new();
    registry.open_document("composite-shape");
    seed(
        &mut registry,
        "composite-shape",
        "background",
        8,
        8,
        [0, 0, 255, 255],
    );

    let shape = PtzLayer {
        id: "shape".to_owned(),
        name: "shape".to_owned(),
        layer_type: LayerType::Shape,
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
        transform: Transform2D {
            x: 1.0,
            y: 1.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
        },
        width: 4.0,
        height: 4.0,
        shape_params: Some(ShapeParams {
            kind: ShapeKind::Rect,
            width: 4.0,
            height: 4.0,
            radius: 0.0,
            fill: ShapeFill {
                kind: ShapeFillKind::Solid,
                color: "#ff0000".to_owned(),
            },
            stroke: ShapeStroke {
                enabled: false,
                color: "#000000".to_owned(),
                width: 1.0,
            },
            arrow_head: false,
        }),
        text_data: None,
    };
    let content = PtzDocumentContent {
        id: "composite-shape".to_owned(),
        name: "Composite shape".to_owned(),
        width: 8.0,
        height: 8.0,
        layers: vec![shape, raster_layer("background", 8.0, 8.0, 1.0, true)],
        selection: None,
    };

    let result = composite_layers(&content, &registry).expect("composite succeeds");
    let mut expected = Vec::with_capacity(8 * 8 * 4);
    for y in 0..8 {
        for x in 0..8 {
            let color = if (1..5).contains(&x) && (1..5).contains(&y) {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            expected.extend_from_slice(&color);
        }
    }
    assert_eq!(result.pixels, expected);
}

#[test]
fn text_layer_composites_its_bitmap_at_the_transform() {
    let data = TextData {
        content: "Ag".to_owned(),
        font_family: "Arial".to_owned(),
        font_size: 16.0,
        line_height: 1.2,
        color: "#000000".to_owned(),
        ..Default::default()
    };
    let bitmap = crate::render::text::rasterize_text(&data, None, 1.0).expect("text rasterizes");

    let layer = |id: &str, layer_type: LayerType, text_data: Option<TextData>| PtzLayer {
        id: id.to_owned(),
        name: id.to_owned(),
        layer_type,
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
        transform: Transform2D {
            x: 1.0,
            y: 1.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
        },
        width: f64::from(bitmap.width),
        height: f64::from(bitmap.height),
        shape_params: None,
        text_data,
    };

    let mut text_registry = PixelStoreRegistry::new();
    text_registry.open_document("composite-text");
    seed(
        &mut text_registry,
        "composite-text",
        "background",
        30,
        30,
        [0, 0, 255, 255],
    );
    let text_content = PtzDocumentContent {
        id: "composite-text".to_owned(),
        name: "composite-text".to_owned(),
        width: 30.0,
        height: 30.0,
        layers: vec![
            layer("text", LayerType::Text, Some(data)),
            raster_layer("background", 30.0, 30.0, 1.0, true),
        ],
        selection: None,
    };

    let mut raster_registry = PixelStoreRegistry::new();
    raster_registry.open_document("composite-raster-twin");
    seed(
        &mut raster_registry,
        "composite-raster-twin",
        "background",
        30,
        30,
        [0, 0, 255, 255],
    );
    raster_registry
        .add_layer(
            "composite-raster-twin",
            "text-raster",
            bitmap.width,
            bitmap.height,
            bitmap.pixels.clone(),
        )
        .expect("text bitmap seeds");
    let raster_content = PtzDocumentContent {
        id: "composite-raster-twin".to_owned(),
        name: "composite-raster-twin".to_owned(),
        width: 30.0,
        height: 30.0,
        layers: vec![
            layer("text-raster", LayerType::Raster, None),
            raster_layer("background", 30.0, 30.0, 1.0, true),
        ],
        selection: None,
    };

    let text_result =
        composite_layers(&text_content, &text_registry).expect("text composite succeeds");
    let raster_result =
        composite_layers(&raster_content, &raster_registry).expect("raster composite succeeds");
    assert_eq!(text_result.pixels, raster_result.pixels);
}

#[test]
fn multilayer_document_matches_canvas2d_oracle() {
    const GOLDEN: &[u8] = include_bytes!("../goldens/multilayer.png");

    let mut registry = PixelStoreRegistry::new();
    registry.open_document("composite-multilayer");
    let mut pattern = Vec::with_capacity(40 * 40 * 4);
    for y in 0..40 {
        for x in 0..40 {
            pattern.extend_from_slice(&[(x * 5) as u8, (y * 5) as u8, 128, 255]);
        }
    }
    registry
        .add_layer("composite-multilayer", "a", 40, 40, pattern)
        .expect("bottom raster seeds");
    registry
        .add_layer(
            "composite-multilayer",
            "b",
            8,
            8,
            [0, 0, 255, 255].repeat(64),
        )
        .expect("blend raster seeds");
    registry
        .add_layer(
            "composite-multilayer",
            "hidden",
            8,
            8,
            [0, 255, 0, 255].repeat(64),
        )
        .expect("hidden raster seeds");
    registry
        .add_layer(
            "composite-multilayer",
            "zero",
            8,
            8,
            [255, 0, 255, 255].repeat(64),
        )
        .expect("zero-opacity raster seeds");

    let text_data = TextData {
        content: "Ag".to_owned(),
        font_family: "Arial".to_owned(),
        font_size: 16.0,
        line_height: 1.2,
        color: "#000000".to_owned(),
        ..Default::default()
    };
    let text_bitmap =
        crate::render::text::rasterize_text(&text_data, None, 1.0).expect("text rasterizes");
    let shape_params = ShapeParams {
        kind: ShapeKind::Rect,
        width: 4.0,
        height: 4.0,
        radius: 0.0,
        fill: ShapeFill {
            kind: ShapeFillKind::Solid,
            color: "#ff0000".to_owned(),
        },
        stroke: ShapeStroke {
            enabled: true,
            color: "#000000".to_owned(),
            width: 1.0,
        },
        arrow_head: false,
    };
    let shape_bitmap =
        crate::render::shape::rasterize_shape(&shape_params).expect("shape rasterizes");

    let mut text = raster_layer(
        "text",
        f64::from(text_bitmap.width),
        f64::from(text_bitmap.height),
        1.0,
        true,
    );
    text.layer_type = LayerType::Text;
    text.text_data = Some(text_data);
    text.transform.x = 1.0;
    text.transform.y = 10.0;

    let mut shape = raster_layer(
        "shape",
        f64::from(shape_bitmap.width),
        f64::from(shape_bitmap.height),
        1.0,
        true,
    );
    shape.layer_type = LayerType::Shape;
    shape.shape_params = Some(shape_params);
    shape.transform.x = 9.0;
    shape.transform.y = 1.0;

    let mut hidden = raster_layer("hidden", 8.0, 8.0, 1.0, false);
    hidden.transform.x = 30.0;
    hidden.transform.y = 30.0;

    let mut zero = raster_layer("zero", 8.0, 8.0, 0.0, true);
    zero.transform.x = 32.0;
    zero.transform.y = 0.0;

    let mut blended = raster_layer("b", 8.0, 8.0, 0.5, true);
    blended.transform.x = 16.0;
    blended.transform.y = 16.0;
    blended.blend_mode = BlendMode::Multiply;

    let bottom = raster_layer("a", 40.0, 40.0, 1.0, true);
    let content = PtzDocumentContent {
        id: "composite-multilayer".to_owned(),
        name: "Composite multilayer".to_owned(),
        width: 40.0,
        height: 40.0,
        layers: vec![text, shape, hidden, zero, blended, bottom],
        selection: None,
    };

    let result = composite_layers(&content, &registry).expect("composite succeeds");
    let golden = image::load_from_memory(GOLDEN)
        .expect("oracle decodes")
        .to_rgba8();
    let (golden_width, golden_height) = golden.dimensions();
    assert_eq!(
        (result.width, result.height),
        (golden_width, golden_height),
        "oracle dimensions"
    );
    let mut max_delta = 0u8;
    for (actual, expected) in result
        .pixels
        .chunks_exact(4)
        .zip(golden.as_raw().chunks_exact(4))
    {
        max_delta = max_delta.max(actual[3].abs_diff(expected[3]));
        for channel in 0..3 {
            let actual_premul = u16::from(actual[channel]) * u16::from(actual[3]) / 255;
            let expected_premul = u16::from(expected[channel]) * u16::from(expected[3]) / 255;
            max_delta = max_delta.max(actual_premul.abs_diff(expected_premul) as u8);
        }
    }
    assert!(
        max_delta <= 27,
        "multi-layer oracle delta {max_delta} exceeds 27"
    );

    let mut without = content.clone();
    without
        .layers
        .retain(|layer| layer.id != "hidden" && layer.id != "zero");
    let baseline = composite_layers(&without, &registry).expect("baseline composite succeeds");
    assert_eq!(
        result.pixels, baseline.pixels,
        "hidden and zero-opacity layers must not change the output"
    );
}

#[test]
fn visible_unsupported_layers_are_rejected() {
    let mut registry = PixelStoreRegistry::new();
    registry.open_document("composite-order");
    seed(
        &mut registry,
        "composite-order",
        "layer",
        4,
        4,
        [1, 2, 3, 255],
    );

    let mut text = raster_layer("layer", 4.0, 4.0, 1.0, true);
    text.layer_type = LayerType::Text;
    assert_eq!(
        composite_layers(&document(vec![text]), &registry).unwrap_err(),
        CompositeError::InvalidText {
            layer_id: "layer".to_owned()
        }
    );

    let mut group = raster_layer("layer", 4.0, 4.0, 1.0, true);
    group.layer_type = LayerType::Group;
    assert_eq!(
        composite_layers(&document(vec![group]), &registry).unwrap_err(),
        CompositeError::UnsupportedLayerType {
            layer_id: "layer".to_owned()
        }
    );

    let mut broken = raster_layer("layer", 4.0, 4.0, 1.0, true);
    broken.transform.scale_x = f64::NAN;
    assert_eq!(
        composite_layers(&document(vec![broken]), &registry).unwrap_err(),
        CompositeError::UnsupportedTransform {
            layer_id: "layer".to_owned()
        }
    );

    let empty = PixelStoreRegistry::new();
    assert_eq!(
        composite_layers(
            &document(vec![raster_layer("layer", 4.0, 4.0, 1.0, true)]),
            &empty
        )
        .unwrap_err(),
        CompositeError::MissingPixels {
            layer_id: "layer".to_owned()
        }
    );
}

#[test]
fn empty_document_composites_to_an_empty_buffer() {
    let registry = PixelStoreRegistry::new();
    let content = PtzDocumentContent {
        id: "composite-order".to_owned(),
        name: "Composite order".to_owned(),
        width: 0.0,
        height: 0.0,
        layers: Vec::new(),
        selection: None,
    };
    let result = composite_layers(&content, &registry).expect("0x0 composites");
    assert_eq!((result.width, result.height), (0, 0));
    assert!(result.pixels.is_empty());
}
