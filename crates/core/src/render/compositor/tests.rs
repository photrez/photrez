use super::*;
use crate::pixel::store::PixelStoreRegistry;
use crate::ptz::content::{BlendMode, LayerType, PtzDocumentContent, PtzLayer, Transform2D};
use crate::ptz::layer_params::{
    ShapeFill, ShapeFillKind, ShapeKind, ShapeParams, ShapeStroke, TextData,
};
use crate::render::text::TEST_FONT;

#[path = "tests/goldens.rs"]
mod goldens;

const GOLDEN_WIDTH: u32 = 4;
const GOLDEN_HEIGHT: u32 = 4;
// Reference compositor capture: bottom red 4x4 opaque, top blue 2x2 at (0,0) with
// opacity 0.5, source-over, read back row-major RGBA.
const SOURCE_OVER_GOLDEN: [u8; 64] = [
    127, 0, 128, 255, 127, 0, 128, 255, 255, 0, 0, 255, 255, 0, 0, 255, //
    127, 0, 128, 255, 127, 0, 128, 255, 255, 0, 0, 255, 255, 0, 0, 255, //
    255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, //
    255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
];

// Reference compositor capture, 4x4: a 2x2 pattern at (1,1) with each flip variant.
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

fn identity_transform() -> Transform2D {
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
fn region_composite_matches_full_output_across_transform_paths() {
    let mut registry = PixelStoreRegistry::new();
    registry.open_document("composite-order");
    registry
        .add_layer(
            "composite-order",
            "top-pattern",
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 180, 0, 0, 255, 128, 255, 255, 0, 220,
            ],
        )
        .expect("pattern layer seeds");
    seed(
        &mut registry,
        "composite-order",
        "bottom",
        4,
        4,
        [30, 80, 150, 192],
    );

    let transforms = [
        Transform2D {
            x: 1.0,
            y: 1.0,
            flip_h: true,
            ..identity_transform()
        },
        Transform2D {
            x: 0.5,
            y: -0.25,
            scale_x: 1.25,
            scale_y: 0.75,
            flip_v: true,
            ..identity_transform()
        },
        Transform2D {
            x: 1.0,
            y: 0.5,
            scale_x: 1.5,
            scale_y: 0.8,
            rotation: 37.0,
            flip_h: true,
            ..identity_transform()
        },
    ];
    let requests = [
        CompositeRegion {
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        },
        CompositeRegion {
            x: 3,
            y: 2,
            width: 5,
            height: 4,
        },
        CompositeRegion {
            x: 7,
            y: 1,
            width: 2,
            height: 2,
        },
    ];

    for transform in transforms {
        let mut top = raster_layer("top-pattern", 2.0, 2.0, 0.7, true);
        top.transform = transform.clone();
        top.blend_mode = BlendMode::Multiply;
        let content = document(vec![top, raster_layer("bottom", 4.0, 4.0, 1.0, true)]);
        let full = composite_layers(&content, &registry).expect("full composite succeeds");

        for request in requests {
            let actual =
                composite_layers_in_region_with_font_data(&content, &registry, None, request)
                    .expect("region composite succeeds");
            let x = request.x.min(full.width);
            let y = request.y.min(full.height);
            let right = request.x.saturating_add(request.width).min(full.width);
            let bottom = request.y.saturating_add(request.height).min(full.height);
            let expected_region = CompositeRegion {
                x,
                y,
                width: right.saturating_sub(x),
                height: bottom.saturating_sub(y),
            };
            assert_eq!(actual.region, expected_region);

            let mut expected = Vec::new();
            for row in expected_region.y..expected_region.y + expected_region.height {
                let start = ((row as usize * full.width as usize) + expected_region.x as usize) * 4;
                let end = start + expected_region.width as usize * 4;
                expected.extend_from_slice(&full.pixels[start..end]);
            }
            assert_eq!(
                actual.pixels, expected,
                "transform {transform:?}, request {request:?}"
            );
        }
    }
}

#[test]
#[ignore = "manual Move preview composition measurement"]
fn move_preview_full_composite_perf_probe() {
    use std::time::Instant;

    const WIDTH: u32 = 4096;
    const HEIGHT: u32 = 1024;
    let doc_id = "move-preview-perf";
    let mut registry = PixelStoreRegistry::new();
    registry.open_document(doc_id);
    seed(
        &mut registry,
        doc_id,
        "selected",
        512,
        512,
        [220, 120, 40, 192],
    );
    seed(
        &mut registry,
        doc_id,
        "middle",
        WIDTH,
        HEIGHT,
        [30, 80, 150, 160],
    );
    seed(
        &mut registry,
        doc_id,
        "bottom",
        WIDTH,
        HEIGHT,
        [20, 40, 60, 255],
    );

    let mut selected = raster_layer("selected", 512.0, 512.0, 1.0, true);
    selected.transform.x = 768.0;
    selected.transform.y = 256.0;
    let mut content = document(vec![
        selected,
        raster_layer("middle", WIDTH as f64, HEIGHT as f64, 1.0, true),
        raster_layer("bottom", WIDTH as f64, HEIGHT as f64, 1.0, true),
    ]);
    content.id = doc_id.to_owned();
    content.width = WIDTH as f64;
    content.height = HEIGHT as f64;

    let mut preview_content = content.clone();
    preview_content.layers[0].transform.x = 832.0;
    preview_content.layers[0].transform.y = 320.0;
    let region = CompositeRegion {
        x: 760,
        y: 248,
        width: 640,
        height: 640,
    };
    let full = composite_layers(&preview_content, &registry).expect("full composite succeeds");
    let partial =
        composite_layers_in_region_with_font_data(&preview_content, &registry, None, region)
            .expect("region composite succeeds");
    assert_eq!(
        (partial.document_width, partial.document_height),
        (WIDTH, HEIGHT)
    );
    assert_eq!(partial.region, region);
    let mut expected = Vec::with_capacity(region.width as usize * region.height as usize * 4);
    for row in region.y..region.y + region.height {
        let start = ((row as usize * WIDTH as usize) + region.x as usize) * 4;
        let end = start + region.width as usize * 4;
        expected.extend_from_slice(&full.pixels[start..end]);
    }
    assert_eq!(partial.pixels, expected);

    for _ in 0..2 {
        let output =
            composite_layers(&preview_content, &registry).expect("full composite succeeds");
        std::hint::black_box(output.pixels.len());
        let output =
            composite_layers_in_region_with_font_data(&preview_content, &registry, None, region)
                .expect("region composite succeeds");
        std::hint::black_box(output.pixels.len());
    }
    let measure_full = || {
        let started = Instant::now();
        let output =
            composite_layers(&preview_content, &registry).expect("full composite succeeds");
        std::hint::black_box(output.pixels.len());
        started.elapsed()
    };
    let measure_region = || {
        let started = Instant::now();
        let output =
            composite_layers_in_region_with_font_data(&preview_content, &registry, None, region)
                .expect("region composite succeeds");
        std::hint::black_box(output.pixels.len());
        started.elapsed()
    };
    let mut full_samples = Vec::with_capacity(9);
    let mut region_samples = Vec::with_capacity(9);
    for sample in 0..9 {
        if sample % 2 == 0 {
            full_samples.push(measure_full());
            region_samples.push(measure_region());
        } else {
            region_samples.push(measure_region());
            full_samples.push(measure_full());
        }
    }
    full_samples.sort_unstable();
    region_samples.sort_unstable();
    let full_sample_ms: Vec<_> = full_samples
        .iter()
        .map(|sample| (sample.as_secs_f64() * 1_000_000.0).round() / 1_000.0)
        .collect();
    let region_sample_ms: Vec<_> = region_samples
        .iter()
        .map(|sample| (sample.as_secs_f64() * 1_000_000.0).round() / 1_000.0)
        .collect();
    eprintln!("MOVE_COMPOSITE_SAMPLES full_ms={full_sample_ms:?} region_ms={region_sample_ms:?}");
    let full_median = full_samples[full_samples.len() / 2];
    let region_median = region_samples[region_samples.len() / 2];
    eprintln!(
        "MOVE_COMPOSITE_COMPARE width={WIDTH} height={HEIGHT} layers=3 region=640x640 n={} full_median_ms={:.3} region_median_ms={:.3} speedup={:.2}x",
        full_samples.len(),
        full_median.as_secs_f64() * 1000.0,
        region_median.as_secs_f64() * 1000.0,
        full_median.as_secs_f64() / region_median.as_secs_f64(),
    );
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

/// The fixture behind the multilayer golden: four seeded rasters (one hidden,
/// one zero-opacity, one blended) plus text and shape layers positioned over
/// them. The text layer renders with the pinned `TEST_FONT` face so the
/// golden is reproducible on every OS; `write_multilayer_golden` regenerates
/// the committed PNG from this fixture.
fn multilayer_fixture() -> (PixelStoreRegistry, PtzDocumentContent) {
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
        font_family: "Inter".to_owned(),
        font_size: 16.0,
        line_height: 1.2,
        color: "#000000".to_owned(),
        ..Default::default()
    };
    let text_bitmap = crate::render::text::rasterize_text(&text_data, Some(TEST_FONT), 1.0)
        .expect("text rasterizes");
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
    (registry, content)
}

/// Pixel-exact against the golden captured on the Windows text path; the
/// pinned-face rasterizer differs by sub-pixel coverage on other platforms.
/// The cross-platform guarantees are `render::text`'s determinism and scale
/// tests, which run everywhere.
#[cfg(windows)]
#[test]
fn multilayer_document_matches_pinned_font_golden() {
    const GOLDEN: &[u8] = include_bytes!("../goldens/multilayer.png");

    let (registry, content) = multilayer_fixture();
    let result = composite_layers_with_font_data(&content, &registry, Some(TEST_FONT))
        .expect("composite succeeds");
    let golden = image::load_from_memory(GOLDEN)
        .expect("golden decodes")
        .to_rgba8();
    let (golden_width, golden_height) = golden.dimensions();
    assert_eq!(
        (result.width, result.height),
        (golden_width, golden_height),
        "golden dimensions"
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
        "multi-layer golden delta {max_delta} exceeds 27"
    );

    let mut without = content.clone();
    without
        .layers
        .retain(|layer| layer.id != "hidden" && layer.id != "zero");
    let baseline = composite_layers_with_font_data(&without, &registry, Some(TEST_FONT))
        .expect("baseline composite succeeds");
    assert_eq!(
        result.pixels, baseline.pixels,
        "hidden and zero-opacity layers must not change the output"
    );
}

/// Regenerate `render/goldens/multilayer.png` from the pinned-font fixture.
/// Run explicitly after an intentional compositor or font-bytes change:
/// `cargo test -p photrez-core --lib -- render::compositor::tests::write_multilayer_golden --ignored`
#[test]
#[ignore = "fixture writer: run explicitly to regenerate the multilayer golden"]
fn write_multilayer_golden() {
    let (registry, content) = multilayer_fixture();
    let result = composite_layers_with_font_data(&content, &registry, Some(TEST_FONT))
        .expect("composite succeeds");
    let image = image::RgbaImage::from_raw(result.width, result.height, result.pixels)
        .expect("composite pixels match the document dimensions");
    image
        .save(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/render/goldens/multilayer.png"
        ))
        .expect("multilayer golden writes");
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
