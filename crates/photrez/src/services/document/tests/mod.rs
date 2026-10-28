use std::io::{Cursor, Write};
use std::path::Path;

use photrez_core::pixel::PixelStoreRegistry;
use photrez_core::ptz::content::{
    BlendMode, LayerType, PtzDocumentContent, PtzLayer, ShapeFill, ShapeFillKind, ShapeKind,
    ShapeParams, ShapeStroke, TextData, Transform2D,
};
use serde_json::{json, Value};
use zip::write::SimpleFileOptions;

use crate::services::error::ServiceError;
use crate::services::file_io::{FileIo, SaveTarget};
use crate::services::ptz_limits::MAX_STRUCTURAL_ENTRIES;

use super::{load_document_candidate, LoadedDocument};

mod save;

const PNG_1X1: [u8; 88] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 1, 98, 58, 81, 161, 241, 31, 0, 0, 0,
    255, 255, 195, 173, 177, 143, 0, 0, 0, 6, 73, 68, 65, 84, 3, 0, 5, 230, 2, 106, 179, 130, 221,
    138, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

struct MemoryFileIo {
    bytes: Vec<u8>,
}

impl FileIo for MemoryFileIo {
    fn approve_path(&self, _raw_path: &Path) -> Result<(), ServiceError> {
        Ok(())
    }

    fn read_approved(&self, _raw_path: &Path, max_bytes: u64) -> Result<Vec<u8>, ServiceError> {
        if self.bytes.len() as u64 > max_bytes {
            return Err(ServiceError::ResourceLimit(
                "test input exceeds limit".into(),
            ));
        }
        Ok(self.bytes.clone())
    }

    fn authorize_save_target(&self, _raw_path: &Path) -> Result<SaveTarget, ServiceError> {
        Err(ServiceError::Validation("not used by loader tests".into()))
    }
}

fn load(bytes: Vec<u8>) -> Result<LoadedDocument, ServiceError> {
    load_document_candidate(&MemoryFileIo { bytes }, Path::new("approved.ptz"))
}

fn rejected<T>(result: Result<T, ServiceError>, message: &str) -> ServiceError {
    match result {
        Ok(_) => panic!("{message}"),
        Err(error) => error,
    }
}

fn raster_layer(id: &str, width: u32, height: u32) -> Value {
    json!({
        "id": id,
        "name": "Layer",
        "type": "raster",
        "visible": true,
        "opacity": 1.0,
        "locked": false,
        "blendMode": "normal",
        "transform": {
            "x": 0.0,
            "y": 0.0,
            "scaleX": 1.0,
            "scaleY": 1.0,
            "rotation": 0.0,
            "flipH": false,
            "flipV": false
        },
        "width": width,
        "height": height
    })
}

fn typed_layer(
    id: &str,
    layer_type: LayerType,
    width: f64,
    height: f64,
    position: (f64, f64),
    shape_params: Option<ShapeParams>,
    text_data: Option<TextData>,
) -> Value {
    serde_json::to_value(PtzLayer {
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
            x: position.0,
            y: position.1,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
        },
        width,
        height,
        shape_params,
        text_data,
    })
    .expect("typed layer serializes")
}

fn png_rgba(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    photrez_core::pixel::export::encode_image(width, height, pixels, "png", 100)
        .expect("test PNG encodes")
}

fn document_json(layers: Vec<Value>, width: u32, height: u32) -> Vec<u8> {
    document_json_for("doc-1", layers, width, height)
}

fn document_json_for(id: &str, layers: Vec<Value>, width: u32, height: u32) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "format": "photrez-ptz",
        "version": 3,
        "id": id,
        "name": "Test",
        "width": width,
        "height": height,
        "layers": layers
    }))
    .expect("document metadata serializes")
}

fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .expect("ZIP entry starts");
        writer.write_all(bytes).expect("ZIP entry is written");
    }
    writer.finish().expect("ZIP archive finishes").into_inner()
}

fn archive_with_empty_entries(count: usize) -> Vec<u8> {
    let names: Vec<_> = (0..count).map(|index| format!("entry-{index}")).collect();
    let empty: &[u8] = &[];
    let entries: Vec<_> = names.iter().map(|name| (name.as_str(), empty)).collect();
    archive(&entries)
}

fn end_record_with_entries(entries: u16) -> Vec<u8> {
    let mut bytes = vec![0; 22];
    bytes[..4].copy_from_slice(b"PK\x05\x06");
    bytes[8..10].copy_from_slice(&entries.to_le_bytes());
    bytes[10..12].copy_from_slice(&entries.to_le_bytes());
    bytes
}

fn zip64_empty_archive() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(98);
    bytes.extend_from_slice(b"PK\x06\x06");
    bytes.extend_from_slice(&44u64.to_le_bytes());
    bytes.extend_from_slice(&45u16.to_le_bytes());
    bytes.extend_from_slice(&45u16.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u64.to_le_bytes());
    bytes.extend_from_slice(&0u64.to_le_bytes());
    bytes.extend_from_slice(&0u64.to_le_bytes());
    bytes.extend_from_slice(&0u64.to_le_bytes());
    bytes.extend_from_slice(b"PK\x06\x07");
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u64.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(b"PK\x05\x06");
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes
}

fn single_raster_archive(layer_id: &str, width: u32, height: u32, png: &[u8]) -> Vec<u8> {
    let metadata = document_json(vec![raster_layer(layer_id, width, height)], width, height);
    let entry_name = format!("layers/{layer_id}.png");
    archive(&[("document.json", &metadata), (&entry_name, png)])
}

fn assert_validation(error: ServiceError) {
    assert_eq!(error.code(), "E_VALIDATION", "{error}");
}

#[test]
fn valid_single_raster_loads_as_a_detached_candidate_with_canonical_rgba() {
    let loaded = load(single_raster_archive("layer-1", 1, 1, &PNG_1X1))
        .expect("single raster candidate loads");
    let layer = loaded
        .registry
        .get_layer(&loaded.document_id, "layer-1")
        .expect("candidate owns its raster");

    assert_eq!(loaded.document_id, "doc-1");
    assert_eq!((loaded.display_width, loaded.display_height), (1, 1));
    assert_eq!(loaded.display_rgba.len(), 4);
    assert_eq!(loaded.display_rgba, layer.pixels);
    assert_eq!(
        loaded
            .registry
            .document_content(&loaded.document_id)
            .expect("typed content was seeded")
            .layers
            .len(),
        1
    );
}

#[test]
fn unsupported_document_format_is_rejected() {
    let mut metadata: Value =
        serde_json::from_slice(&document_json(vec![raster_layer("layer-1", 1, 1)], 1, 1))
            .expect("fixture metadata parses");
    metadata["format"] = json!("other-format");
    let metadata = serde_json::to_vec(&metadata).expect("metadata serializes");
    let bytes = archive(&[
        ("document.json", &metadata),
        ("layers/layer-1.png", &PNG_1X1),
    ]);

    assert_validation(rejected(
        load(bytes),
        "unsupported document format is refused",
    ));
}

#[test]
fn unsupported_document_version_is_rejected() {
    let mut metadata: Value =
        serde_json::from_slice(&document_json(vec![raster_layer("layer-1", 1, 1)], 1, 1))
            .expect("fixture metadata parses");
    metadata["version"] = json!(4);
    let metadata = serde_json::to_vec(&metadata).expect("metadata serializes");
    let bytes = archive(&[
        ("document.json", &metadata),
        ("layers/layer-1.png", &PNG_1X1),
    ]);

    assert_validation(rejected(
        load(bytes),
        "unsupported document version is refused",
    ));
}

#[test]
fn full_document_snapshot_preserves_session_fields() {
    let mut metadata: Value =
        serde_json::from_slice(&document_json(vec![raster_layer("layer-1", 1, 1)], 1, 1))
            .expect("fixture metadata parses");
    metadata["activeLayerId"] = json!("layer-1");
    metadata["selection"] = json!({
        "x": 1.0,
        "y": 2.0,
        "width": 3.0,
        "height": 4.0,
        "angle": 0.5,
        "shape": null,
        "inverted": true
    });
    metadata["viewport"] = json!({
        "panX": 11.5,
        "panY": -7.25,
        "zoom": 1.75,
        "rotation": 0.25
    });
    metadata["dirty"] = json!(true);
    let metadata = serde_json::to_vec(&metadata).expect("metadata serializes");
    let bytes = archive(&[
        ("document.json", &metadata),
        ("layers/layer-1.png", &PNG_1X1),
    ]);
    let loaded = load(bytes).expect("valid single-raster archive loads");
    let snapshot = loaded.document_snapshot();

    assert_eq!(snapshot.active_layer_id.as_deref(), Some("layer-1"));
    assert_eq!(
        snapshot.selection.as_ref().map(|selection| selection.angle),
        Some(0.5)
    );
    assert_eq!(snapshot.viewport.pan_x, 11.5);
    assert_eq!(snapshot.viewport.pan_y, -7.25);
    assert_eq!(snapshot.viewport.zoom, 1.75);
    assert_eq!(snapshot.viewport.rotation, 0.25);
    assert!(snapshot.dirty);
}

#[test]
fn malformed_archive_is_rejected() {
    assert_validation(rejected(
        load(b"not a ZIP archive".to_vec()),
        "invalid ZIP is refused",
    ));
}

#[test]
fn structural_entry_limit_is_checked_before_zip_archive_construction() {
    let entries = u16::try_from(MAX_STRUCTURAL_ENTRIES + 1).expect("entry cap fits in EOCD");
    let error = rejected(
        load(end_record_with_entries(entries)),
        "over-limit EOCD is rejected by preflight",
    );
    assert_eq!(error.code(), "E_RESOURCE_LIMIT", "{error}");
}

#[test]
fn earlier_invalid_end_record_fallback_is_rejected() {
    let mut bytes = single_raster_archive("layer-1", 1, 1, &PNG_1X1);
    let original_end_record = bytes.len() - 22;
    let fake_end_record = end_record_with_entries(2);
    bytes.extend_from_slice(&fake_end_record);
    bytes[original_end_record + 20..original_end_record + 22]
        .copy_from_slice(&u16::try_from(fake_end_record.len()).unwrap().to_le_bytes());

    assert_eq!(
        zip::ZipArchive::new(Cursor::new(bytes.as_slice()))
            .expect("ZIP reader falls back to the earlier valid directory")
            .len(),
        2
    );
    let error = rejected(load(bytes), "ambiguous EOCD candidates are refused");
    assert_validation(error);
}

#[test]
fn end_record_at_zip_parser_window_boundary_is_rejected_before_fallback() {
    let mut bytes = archive_with_empty_entries(MAX_STRUCTURAL_ENTRIES + 1);
    let real_end_record = bytes.len() - 22;
    let mut fake_end_record = end_record_with_entries(2);
    fake_end_record[16..20].copy_from_slice(&(u32::MAX - 1).to_le_bytes());
    let padding_size = 65_536 - fake_end_record.len();
    bytes.resize(bytes.len() + padding_size, 0);
    bytes.extend_from_slice(&fake_end_record);
    assert_eq!(bytes.len() - real_end_record, 65_558);

    assert_eq!(
        zip::ZipArchive::new(Cursor::new(bytes.as_slice()))
            .expect("ZIP reader falls back to the earlier over-limit directory")
            .len(),
        MAX_STRUCTURAL_ENTRIES + 1
    );
    let error = rejected(
        load(bytes),
        "ambiguous EOCD candidates are refused before ZIP fallback",
    );
    assert!(
        error.to_string().contains("ambiguous end records"),
        "{error}"
    );
    assert_validation(error);
}

#[test]
fn end_record_at_zip_parser_x64_boundary_is_rejected_before_fallback() {
    const ZIP_9_0_4_X64_EOCD_BOUNDARY_BYTES: usize = 32 + 4 + 65_536;

    let mut bytes = archive_with_empty_entries(MAX_STRUCTURAL_ENTRIES + 1);
    let real_end_record = bytes.len() - 22;
    let mut fake_end_record = end_record_with_entries(2);
    fake_end_record[16..20].copy_from_slice(&(u32::MAX - 1).to_le_bytes());
    let padding_size = ZIP_9_0_4_X64_EOCD_BOUNDARY_BYTES - 22 - fake_end_record.len();
    bytes.resize(bytes.len() + padding_size, 0);
    bytes.extend_from_slice(&fake_end_record);
    assert_eq!(
        bytes.len() - real_end_record,
        ZIP_9_0_4_X64_EOCD_BOUNDARY_BYTES
    );

    assert_eq!(
        zip::ZipArchive::new(Cursor::new(bytes.as_slice()))
            .expect("ZIP reader falls back to the earlier over-limit directory")
            .len(),
        MAX_STRUCTURAL_ENTRIES + 1
    );
    let error = rejected(
        load(bytes),
        "ambiguous EOCD candidates are refused before ZIP fallback",
    );
    assert!(
        error.to_string().contains("ambiguous end records"),
        "{error}"
    );
    assert_validation(error);
}

#[test]
fn zip64_archives_fail_closed_before_loading() {
    let bytes = zip64_empty_archive();
    assert_eq!(
        zip::ZipArchive::new(Cursor::new(bytes.as_slice()))
            .expect("fixture is a valid empty ZIP64 archive")
            .len(),
        0
    );
    let error = rejected(load(bytes), "ZIP64 archive is refused");
    assert!(error.to_string().contains("ZIP64"), "{error}");
    assert_validation(error);

    let error = rejected(
        load(end_record_with_entries(u16::MAX)),
        "saturated EOCD count is refused without a ZIP64 locator",
    );
    assert!(error.to_string().contains("ZIP64"), "{error}");
    assert_validation(error);
}

#[test]
fn missing_document_metadata_is_rejected() {
    assert_validation(rejected(
        load(archive(&[("layers/layer-1.png", &PNG_1X1)])),
        "document metadata is required",
    ));
}

#[test]
fn missing_raster_entry_is_rejected() {
    let metadata = document_json(vec![raster_layer("layer-1", 1, 1)], 1, 1);
    assert_validation(rejected(
        load(archive(&[("document.json", &metadata)])),
        "raster membership must be complete",
    ));
}

#[test]
fn duplicate_document_and_raster_entries_are_rejected() {
    let metadata = document_json(vec![raster_layer("layer-1", 1, 1)], 1, 1);
    let mut duplicate_document =
        archive(&[("document.json", &metadata), ("documenx.json", &metadata)]);
    patch_central_directory_name(&mut duplicate_document, "documenx.json", "document.json");
    assert_validation(rejected(
        load(duplicate_document),
        "duplicate metadata is refused",
    ));

    let mut duplicate_layer = archive(&[
        ("document.json", &metadata),
        ("layers/layer-1.png", &PNG_1X1),
        ("layers/layer-2.png", &PNG_1X1),
    ]);
    patch_central_directory_name(
        &mut duplicate_layer,
        "layers/layer-2.png",
        "layers/layer-1.png",
    );
    assert_validation(rejected(
        load(duplicate_layer),
        "duplicate raster is refused",
    ));
}

#[test]
fn unexpected_archive_entry_is_rejected() {
    let metadata = document_json(vec![raster_layer("layer-1", 1, 1)], 1, 1);
    let bytes = archive(&[
        ("document.json", &metadata),
        ("layers/layer-1.png", &PNG_1X1),
        ("notes.txt", b"unexpected"),
    ]);
    assert_validation(rejected(load(bytes), "unexpected entries are refused"));
}

#[test]
fn traversal_and_absolute_entry_names_are_rejected() {
    for name in [
        "../escape.png",
        "/document.json",
        "C:/escape.png",
        "layers/../escape.png",
    ] {
        assert_validation(rejected(
            load(archive(&[(name, b"entry")])),
            &format!("unsafe entry {name:?} was accepted"),
        ));
    }
}

#[test]
fn invalid_layer_id_is_rejected_before_registry_mutation() {
    let mut layer = raster_layer("../escape", 1, 1);
    layer["id"] = json!("../escape");
    let metadata = document_json(vec![layer], 1, 1);
    assert_validation(rejected(
        load(archive(&[("document.json", &metadata)])),
        "invalid layer ID is refused",
    ));
}

#[test]
fn metadata_and_png_dimensions_must_match() {
    assert_validation(rejected(
        load(single_raster_archive("layer-1", 2, 1, &PNG_1X1)),
        "PNG dimensions disagree with metadata",
    ));
}

#[test]
fn invalid_png_header_is_rejected() {
    assert_validation(rejected(
        load(single_raster_archive("layer-1", 1, 1, b"not a PNG")),
        "invalid PNG header is refused",
    ));
}

#[test]
fn metadata_byte_limit_is_enforced() {
    let mut metadata: Value =
        serde_json::from_slice(&document_json(vec![raster_layer("layer-1", 1, 1)], 1, 1))
            .expect("fixture metadata parses");
    metadata["padding"] = json!("x".repeat(1_048_577));
    let metadata = serde_json::to_vec(&metadata).expect("padded metadata serializes");
    assert_eq!(
        rejected(
            load(archive(&[("document.json", &metadata)])),
            "oversized metadata is refused",
        )
        .code(),
        "E_RESOURCE_LIMIT"
    );
}

#[test]
fn aggregate_decompressed_entry_budget_is_enforced_from_zip_metadata() {
    let mut bytes = single_raster_archive("layer-1", 1, 1, &PNG_1X1);
    patch_central_directory_size(&mut bytes, "layers/layer-1.png", 2_147_483_649);
    assert_eq!(
        rejected(
            load(bytes),
            "oversized declared entry is refused before decompression",
        )
        .code(),
        "E_RESOURCE_LIMIT"
    );
}

#[test]
fn decoded_rgba_budget_is_enforced_before_composition_or_decode() {
    let metadata = document_json(
        vec![
            raster_layer("layer-a", 16_384, 16_384),
            raster_layer("layer-b", 16_384, 16_384),
        ],
        16_384,
        16_384,
    );
    let bytes = archive(&[
        ("document.json", &metadata),
        ("layers/layer-a.png", b"deferred until budget passes"),
        ("layers/layer-b.png", b"deferred until budget passes"),
    ]);
    assert_eq!(
        rejected(
            load(bytes),
            "decoded RGBA aggregate exceeds the document limit",
        )
        .code(),
        "E_RESOURCE_LIMIT"
    );
}

#[test]
fn multiple_raster_layers_composite_in_content_order() {
    let top = png_rgba(1, 1, &[0, 0, 255, 255]);
    let bottom = png_rgba(2, 1, &[255, 0, 0, 255, 255, 0, 0, 255]);
    let metadata = document_json(
        vec![
            raster_layer("layer-top", 1, 1),
            raster_layer("layer-bottom", 2, 1),
        ],
        2,
        1,
    );
    let loaded = load(archive(&[
        ("document.json", &metadata),
        ("layers/layer-top.png", &top),
        ("layers/layer-bottom.png", &bottom),
    ]))
    .expect("two raster layers load and composite");

    let (width, height, rgba) = loaded.canonical_rgba();
    assert_eq!((width, height), (2, 1));
    assert_eq!(rgba, &[0, 0, 255, 255, 255, 0, 0, 255][..]);
    assert_eq!(
        loaded
            .content_snapshot()
            .expect("content is seeded")
            .layers
            .len(),
        2
    );
    assert!(loaded
        .registry
        .get_layer(&loaded.document_id, "layer-bottom")
        .is_some());
}

#[test]
fn shape_layers_render_through_the_production_loader() {
    let background = png_rgba(8, 8, &[0, 0, 255, 255].repeat(64));
    let shape = typed_layer(
        "shape",
        LayerType::Shape,
        4.0,
        4.0,
        (1.0, 1.0),
        Some(ShapeParams {
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
        None,
    );
    let metadata = document_json(vec![shape, raster_layer("background", 8, 8)], 8, 8);
    let loaded = load(archive(&[
        ("document.json", &metadata),
        ("layers/background.png", &background),
    ]))
    .expect("a shape layer loads and renders");

    let (width, height, rgba) = loaded.canonical_rgba();
    assert_eq!((width, height), (8, 8));
    for y in 0..8 {
        for x in 0..8 {
            let expected = if (1..5).contains(&x) && (1..5).contains(&y) {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            let offset = (y * 8 + x) * 4;
            assert_eq!(&rgba[offset..offset + 4], expected, "pixel ({x},{y})");
        }
    }
}

#[test]
fn text_layers_render_through_the_production_loader() {
    let background = png_rgba(30, 30, &[0, 0, 255, 255].repeat(900));
    let text = typed_layer(
        "text",
        LayerType::Text,
        24.0,
        24.0,
        (1.0, 1.0),
        None,
        Some(TextData {
            content: "Ag".to_owned(),
            font_family: "Arial".to_owned(),
            font_size: 16.0,
            line_height: 1.2,
            color: "#000000".to_owned(),
            ..Default::default()
        }),
    );
    let metadata = document_json(vec![text, raster_layer("background", 30, 30)], 30, 30);
    let loaded = load(archive(&[
        ("document.json", &metadata),
        ("layers/background.png", &background),
    ]))
    .expect("a text layer loads and renders");

    let (width, height, rgba) = loaded.canonical_rgba();
    assert_eq!((width, height), (30, 30));
    let ink = rgba
        .chunks_exact(4)
        .filter(|pixel| pixel[0] < 100 && pixel[1] < 100 && pixel[2] < 100)
        .count();
    assert!(ink > 0, "text rasterization produced no ink");
}

#[test]
fn failed_candidate_load_does_not_mutate_a_caller_owned_registry() {
    let mut active = PixelStoreRegistry::new();
    active.open_document("active");
    active
        .add_layer("active", "active-layer", 1, 1, vec![1, 2, 3, 4])
        .expect("active pixels seed");
    let content = PtzDocumentContent::from_ptz_document_json(
        std::str::from_utf8(&document_json_for(
            "active",
            vec![raster_layer("active-layer", 1, 1)],
            1,
            1,
        ))
        .expect("metadata is UTF-8"),
    )
    .expect("active content parses");
    active
        .seed_document("active", content)
        .expect("active document is seeded");
    let content_before = active
        .document_content("active")
        .expect("active content exists")
        .clone();
    let pixels_before = active
        .get_layer("active", "active-layer")
        .expect("active layer exists")
        .pixels
        .clone();

    assert_validation(rejected(
        load(b"cancel-like rejection".to_vec()),
        "candidate is rejected",
    ));
    assert_eq!(active.document_content("active"), Some(&content_before));
    assert_eq!(
        active
            .get_layer("active", "active-layer")
            .expect("active layer remains")
            .pixels,
        pixels_before
    );
}

#[test]
fn imported_image_builds_a_background_layer_document() {
    let loaded = LoadedDocument::imported_image(
        "import-1".to_owned(),
        "photo.png".to_owned(),
        1,
        1,
        vec![1, 2, 3, 255],
    )
    .expect("imported pixels seed");

    assert_eq!(loaded.document_id(), "import-1");
    assert_eq!(loaded.canonical_rgba(), (1, 1, &[1, 2, 3, 255][..]));
    let content = loaded
        .content_snapshot()
        .expect("imported content is seeded");
    assert_eq!(content.name, "photo.png");
    assert_eq!(content.layers.len(), 1);
    assert_eq!(content.layers[0].id, "background");
    assert_eq!(content.layers[0].is_background, Some(true));
    assert_eq!(loaded.document_snapshot().name, "photo.png");
}

#[test]
fn text_layers_render_with_the_bundled_inter_face() {
    let background = png_rgba(30, 30, &[0, 0, 255, 255].repeat(900));
    let text = typed_layer(
        "text",
        LayerType::Text,
        24.0,
        24.0,
        (1.0, 1.0),
        None,
        Some(TextData {
            content: "Inter".to_owned(),
            font_family: "Inter".to_owned(),
            font_size: 16.0,
            line_height: 1.2,
            color: "#000000".to_owned(),
            ..Default::default()
        }),
    );
    let metadata = document_json(vec![text, raster_layer("background", 30, 30)], 30, 30);
    let loaded = load(archive(&[
        ("document.json", &metadata),
        ("layers/background.png", &background),
    ]))
    .expect("text with the bundled face loads and renders");

    let ink = loaded
        .canonical_rgba()
        .2
        .chunks_exact(4)
        .filter(|pixel| pixel[0] < 100 && pixel[1] < 100 && pixel[2] < 100)
        .count();
    assert!(ink > 0, "the bundled Inter face produced no ink");
}

#[test]
fn document_dimensions_above_the_product_bound_are_rejected() {
    let metadata = document_json(vec![raster_layer("layer-1", 1, 1)], 16_385, 1);
    let bytes = archive(&[
        ("document.json", &metadata),
        ("layers/layer-1.png", &PNG_1X1),
    ]);
    let error = rejected(load(bytes), "oversized document is rejected");
    assert_eq!(error.code(), "E_VALIDATION", "{error}");
}

#[test]
fn zero_by_zero_empty_raster_is_preserved_without_a_png_entry() {
    let metadata = document_json(vec![raster_layer("empty", 0, 0)], 0, 0);
    let loaded = load(archive(&[("document.json", &metadata)]))
        .expect("core accepts a zero-sized empty raster");
    let layer = loaded
        .registry
        .get_layer(&loaded.document_id, "empty")
        .expect("empty canonical layer exists");

    assert_eq!((layer.width, layer.height), (0, 0));
    assert!(layer.pixels.is_empty());
    assert!(loaded.display_rgba.is_empty());
}

fn patch_central_directory_size(bytes: &mut [u8], entry_name: &str, size: u32) {
    let mut offset = 0;
    while offset + 46 <= bytes.len() {
        if &bytes[offset..offset + 4] != b"PK\x01\x02" {
            offset += 1;
            continue;
        }
        let name_len = u16::from_le_bytes([bytes[offset + 28], bytes[offset + 29]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[offset + 30], bytes[offset + 31]]) as usize;
        let comment_len = u16::from_le_bytes([bytes[offset + 32], bytes[offset + 33]]) as usize;
        let name_start = offset + 46;
        let name_end = name_start + name_len;
        if bytes.get(name_start..name_end) == Some(entry_name.as_bytes()) {
            bytes[offset + 24..offset + 28].copy_from_slice(&size.to_le_bytes());
            return;
        }
        offset = name_end + extra_len + comment_len;
    }
    panic!("central-directory entry {entry_name:?} was not found");
}

fn patch_central_directory_name(bytes: &mut [u8], old_name: &str, new_name: &str) {
    assert_eq!(old_name.len(), new_name.len());
    let mut offset = 0;
    while offset + 46 <= bytes.len() {
        if &bytes[offset..offset + 4] != b"PK\x01\x02" {
            offset += 1;
            continue;
        }
        let name_len = u16::from_le_bytes([bytes[offset + 28], bytes[offset + 29]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[offset + 30], bytes[offset + 31]]) as usize;
        let comment_len = u16::from_le_bytes([bytes[offset + 32], bytes[offset + 33]]) as usize;
        let name_start = offset + 46;
        let name_end = name_start + name_len;
        if bytes.get(name_start..name_end) == Some(old_name.as_bytes()) {
            bytes[name_start..name_end].copy_from_slice(new_name.as_bytes());
            let local_offset =
                u32::from_le_bytes(bytes[offset + 42..offset + 46].try_into().unwrap()) as usize;
            let local_name_len =
                u16::from_le_bytes([bytes[local_offset + 26], bytes[local_offset + 27]]) as usize;
            let local_name_start = local_offset + 30;
            let local_name_end = local_name_start + local_name_len;
            assert_eq!(
                bytes.get(local_name_start..local_name_end),
                Some(old_name.as_bytes())
            );
            bytes[local_name_start..local_name_end].copy_from_slice(new_name.as_bytes());
            return;
        }
        offset = name_end + extra_len + comment_len;
    }
    panic!("central-directory entry {old_name:?} was not found");
}

#[test]
fn blank_document_is_800x600_opaque_white() {
    let (width, height, rgba) = super::blank_white_document();
    assert_eq!((width, height), (800, 600));
    assert_eq!(rgba, vec![255u8; 800 * 600 * 4]);
}
