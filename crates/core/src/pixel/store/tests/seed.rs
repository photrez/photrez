use super::{reg, tile};
use crate::pixel::store::PixelStoreRegistry;
use crate::pixel::{HistoryDepth, PixelStoreBytes};
use crate::ptz::content::{BlendMode, LayerType, PtzDocumentContent, PtzLayer, Transform2D};

fn seed_content(doc_id: &str, layer_id: &str, width: f64, height: f64) -> PtzDocumentContent {
    PtzDocumentContent {
        id: doc_id.to_string(),
        name: "Seeded document".to_string(),
        width,
        height,
        layers: vec![PtzLayer {
            id: layer_id.to_string(),
            name: "Raster layer".to_string(),
            layer_type: LayerType::Raster,
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
        }],
        selection: None,
    }
}

fn nondefault_content(doc_id: &str) -> PtzDocumentContent {
    let mut content = seed_content(doc_id, "pixels", 2.0, 1.0);
    content.name = "Nondefault document".to_string();
    let raster = &mut content.layers[0];
    raster.name = "Locked multiply layer".to_string();
    raster.visible = false;
    raster.opacity = 0.375;
    raster.locked = true;
    raster.is_background = Some(true);
    raster.lock_transparency = Some(true);
    raster.lock_position = Some(true);
    raster.lock_rotation = Some(false);
    raster.has_adjustments = Some(true);
    raster.blend_mode = BlendMode::Multiply;
    raster.transform = Transform2D {
        x: 1.25,
        y: -3.5,
        scale_x: 1.5,
        scale_y: 0.75,
        rotation: 27.0,
        flip_h: true,
        flip_v: false,
    };

    let mut group = raster.clone();
    group.id = "group".to_string();
    group.name = "Group metadata".to_string();
    group.layer_type = LayerType::Group;
    group.width = 0.0;
    group.height = 0.0;
    group.opacity = 0.625;
    content.layers.push(group);
    content
}

fn assert_nondefault_metadata(content: &PtzDocumentContent, doc_id: &str) {
    assert_eq!(content.id, doc_id);
    assert_eq!(content.name, "Nondefault document");
    let raster = &content.layers[0];
    assert_eq!(raster.name, "Locked multiply layer");
    assert!(!raster.visible);
    assert_eq!(raster.opacity, 0.375);
    assert!(raster.locked);
    assert_eq!(raster.is_background, Some(true));
    assert_eq!(raster.lock_transparency, Some(true));
    assert_eq!(raster.lock_position, Some(true));
    assert_eq!(raster.lock_rotation, Some(false));
    assert_eq!(raster.has_adjustments, Some(true));
    assert_eq!(raster.blend_mode, BlendMode::Multiply);
    assert_eq!(
        raster.transform,
        Transform2D {
            x: 1.25,
            y: -3.5,
            scale_x: 1.5,
            scale_y: 0.75,
            rotation: 27.0,
            flip_h: true,
            flip_v: false,
        }
    );
    let group = &content.layers[1];
    assert_eq!(group.id, "group");
    assert_eq!(group.layer_type, LayerType::Group);
    assert_eq!(group.opacity, 0.625);
}

#[derive(Debug, PartialEq, Eq)]
struct ObservedLayer {
    id: String,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    epoch: u64,
}

#[derive(Debug, PartialEq)]
struct SeedSnapshot {
    layers: Vec<ObservedLayer>,
    store_bytes: Option<PixelStoreBytes>,
    cursor: Option<usize>,
    version: Option<u64>,
    history: Option<HistoryDepth>,
    content: Option<PtzDocumentContent>,
}

fn snapshot(r: &PixelStoreRegistry, layer_ids: &[&str]) -> SeedSnapshot {
    const DOC: &str = "seed-regression";
    SeedSnapshot {
        layers: layer_ids
            .iter()
            .map(|id| {
                let layer = r.get_layer(DOC, id).expect("observed layer exists");
                ObservedLayer {
                    id: (*id).to_string(),
                    width: layer.width,
                    height: layer.height,
                    pixels: layer.pixels.clone(),
                    epoch: r.get_epoch(DOC, id).expect("observed epoch exists"),
                }
            })
            .collect(),
        store_bytes: r.get_store_bytes(DOC),
        cursor: r.get_history_cursor(DOC),
        version: r.get_history_version(DOC),
        history: r.get_history_depth(DOC),
        content: r.document_content(DOC).cloned(),
    }
}

fn seeded_nondefault_baseline() -> (PixelStoreRegistry, PtzDocumentContent) {
    const DOC: &str = "seed-regression";
    let mut r = reg();
    r.add_layer(DOC, "pixels", 2, 1, vec![0; 8])
        .expect("candidate pixels");
    let content = nondefault_content(DOC);
    r.seed_document(DOC, content.clone())
        .expect("nondefault baseline seed");
    assert_nondefault_metadata(r.document_content(DOC).expect("seeded content"), DOC);
    assert!(r
        .apply_pixel_patch(
            DOC,
            "pixels",
            vec![tile(0, 0, 2, 1, 0)],
            vec![tile(0, 0, 2, 1, 9)],
        )
        .is_some());
    (r, content)
}

#[test]
fn seed_document_succeeds_for_pristine_empty_layer_candidate() {
    let mut r = reg();
    r.add_layer("empty-doc", "empty-layer", 0, 0, Vec::new())
        .unwrap();
    let content = seed_content("empty-doc", "empty-layer", 0.0, 0.0);

    r.seed_document("empty-doc", content.clone()).unwrap();

    assert_eq!(r.get_history_cursor("empty-doc"), Some(0));
    let readback = r.document_content("empty-doc").unwrap();
    assert_eq!(readback.id, content.id);
    assert_eq!(readback.layers[0].id, content.layers[0].id);
    assert_eq!(readback.layers[0].width, 0.0);
    let pixels = r.get_layer("empty-doc", "empty-layer").unwrap();
    assert_eq!((pixels.width, pixels.height), (0, 0));
    assert!(pixels.pixels.is_empty());
}

#[test]
fn seed_document_rejects_id_and_dimension_mismatches_without_mutation() {
    let mut r = reg();
    r.add_layer("seed-doc", "pixels", 2, 1, vec![0; 8]).unwrap();
    assert!(r
        .apply_pixel_patch(
            "seed-doc",
            "pixels",
            vec![tile(0, 0, 2, 1, 0)],
            vec![tile(0, 0, 2, 1, 7)],
        )
        .is_some());

    let before_pixels = r.get_layer("seed-doc", "pixels").unwrap().pixels.clone();
    let before_cursor = r.get_history_cursor("seed-doc");
    let before_version = r.get_history_version("seed-doc");
    let before_depth = r.get_history_depth("seed-doc").unwrap().total_depth;
    let before_bytes = r.get_store_bytes("seed-doc").unwrap();
    let mut wrong_layer_id = seed_content("seed-doc", "pixels", 2.0, 1.0);
    wrong_layer_id.layers[0].id = "other-layer".to_string();
    let mut wrong_dimensions = seed_content("seed-doc", "pixels", 2.0, 1.0);
    wrong_dimensions.layers[0].width = 3.0;
    let mut wrong_document_id = seed_content("seed-doc", "pixels", 2.0, 1.0);
    wrong_document_id.id = "other-doc".to_string();

    for invalid in [wrong_layer_id, wrong_dimensions, wrong_document_id] {
        assert!(r.seed_document("seed-doc", invalid).is_err());
        assert_eq!(
            r.get_layer("seed-doc", "pixels").unwrap().pixels,
            before_pixels
        );
        assert_eq!(r.get_history_cursor("seed-doc"), before_cursor);
        assert_eq!(r.get_history_version("seed-doc"), before_version);
        assert_eq!(
            r.get_history_depth("seed-doc").unwrap().total_depth,
            before_depth
        );
        assert_eq!(r.get_store_bytes("seed-doc"), Some(before_bytes.clone()));
        assert!(r.document_content("seed-doc").is_none());
    }
}

#[test]
fn document_content_reads_back_typed_seed() {
    let mut r = reg();
    r.add_layer("readback-doc", "raster", 2, 1, vec![3; 8])
        .unwrap();
    let content = seed_content("readback-doc", "raster", 2.0, 1.0);

    r.seed_document("readback-doc", content.clone()).unwrap();

    let readback = r.document_content("readback-doc").unwrap();
    assert_eq!(readback.id, content.id);
    assert_eq!(readback.name, content.name);
    assert_eq!(readback.layers[0].id, content.layers[0].id);
    assert_eq!(readback.layers[0].width, content.layers[0].width);
    assert_eq!(&readback.layers[0].layer_type, &LayerType::Raster);
}

#[test]
fn seed_document_rejects_duplicate_ids_without_mutating_seeded_metadata() {
    const DOC: &str = "seed-regression";
    let (mut r, mut duplicate) = seeded_nondefault_baseline();
    duplicate.layers.push(duplicate.layers[0].clone());
    let before = snapshot(&r, &["pixels"]);

    assert!(r.seed_document(DOC, duplicate).is_err());

    assert_eq!(snapshot(&r, &["pixels"]), before);
}

#[test]
fn seed_document_rejects_extra_pixel_membership_without_mutating_seeded_metadata() {
    const DOC: &str = "seed-regression";
    let (mut r, content) = seeded_nondefault_baseline();
    r.add_layer(DOC, "orphan", 1, 1, vec![4; 4])
        .expect("extra pixel layer");
    let before = snapshot(&r, &["pixels", "orphan"]);

    assert!(r.seed_document(DOC, content).is_err());

    assert_eq!(snapshot(&r, &["pixels", "orphan"]), before);
}

#[test]
fn seed_document_rejects_non_raster_pixel_membership_without_mutating_seeded_metadata() {
    const DOC: &str = "seed-regression";
    let (mut r, content) = seeded_nondefault_baseline();
    r.add_layer(DOC, "group", 1, 1, vec![4; 4])
        .expect("non-raster pixel layer");
    let before = snapshot(&r, &["pixels", "group"]);

    assert!(r.seed_document(DOC, content).is_err());

    assert_eq!(snapshot(&r, &["pixels", "group"]), before);
}

#[test]
fn document_content_round_trips_nondefault_metadata() {
    let (r, _) = seeded_nondefault_baseline();

    assert_nondefault_metadata(
        r.document_content("seed-regression")
            .expect("nondefault content readback"),
        "seed-regression",
    );
}
