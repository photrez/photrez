// Golden comparisons: layer order, opacity and visibility against source-over, every
// blend mode, integer translations and flips, and transformed layers within rounding.
use super::super::*;
use super::*;
use crate::test_input::MapInput;
use photrez_document::content::{BlendMode, PtzDocumentContent};
#[test]
fn two_layer_order_opacity_and_visibility_match_the_source_over_golden() {
    let mut registry = MapInput::default();
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
        result.pixels, SOURCE_OVER_GOLDEN,
        "CPU composite must match the 0.1 source-over capture"
    );
}

#[test]
fn all_blend_modes_match_reference_goldens_within_rounding() {
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
        let mut registry = MapInput::default();
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
        // unit because the 0.1 compositor blends in premultiplied 8-bit. Never widen
        // this bound: a larger delta is a formula bug, not rounding.
        assert!(
            delta <= 1,
            "{mode:?} at opacity {opacity}: got {:?}, expected {expected:?}, max delta {delta}",
            result.pixels
        );
    }
}

#[test]
fn integer_translation_and_flips_match_reference_goldens() {
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
        let mut registry = MapInput::default();
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
fn transformed_layers_match_reference_goldens_within_one_unit() {
    let pattern: [u8; 16] = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
    ];
    // The four axis-aligned cases are byte-exact. At 45 degrees the 0.1 build
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
        let mut registry = MapInput::default();
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
