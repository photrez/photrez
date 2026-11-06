use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::services::document;
use crate::services::error::ServiceError;
use crate::services::file_io::{FileIo, LocalFileIo, SaveTarget};
use crate::services::ptz_limits::{
    self, ArchiveBudget, MAX_ARCHIVE_ENTRY_BYTES, MAX_DOCUMENT_DECODED_RGBA_BYTES,
    MAX_DOCUMENT_JSON_BYTES, MAX_LAYER_COUNT, MAX_LAYER_DIMENSION, MAX_STRUCTURAL_ENTRIES,
};
use crate::services::save_stream::SaveStream;
use crate::services::settings::{FileSettings, SettingsStore};
use photrez_core::document::content::PtzDocumentContent;

static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(1);

struct TestDir(PathBuf);

impl TestDir {
    fn new(label: &str) -> Self {
        let id = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "photrez-invalid-input-{}-{label}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("test directory is created");
        // Link-free spelling: the file boundary rejects a path behind a link.
        Self(fs::canonicalize(&path).unwrap_or(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn assert_error_code<T>(result: Result<T, ServiceError>, expected: &str) {
    let Err(error) = result else {
        panic!("expected {expected}");
    };
    assert_eq!(error.code(), expected, "{error}");
}

fn save_target(directory: &Path, name: &str) -> SaveTarget {
    LocalFileIo::new(directory, directory.join("approvals.json"))
        .expect("file service opens")
        .authorize_save_target(&directory.join(name))
        .expect("cache target is authorized")
}

fn png_1x1() -> Vec<u8> {
    photrez_core::pixel::export::encode_image(1, 1, &[255, 0, 0, 255], "png", 100)
        .expect("1x1 PNG encodes")
}

fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .expect("ZIP entry starts");
        writer.write_all(bytes).expect("ZIP entry is written");
    }
    writer.finish().expect("ZIP archive finishes").into_inner()
}

fn raster_document_json(id: &str, width: u32, height: u32) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "format": "photrez-ptz",
        "version": 3,
        "id": "test-document",
        "name": "Test",
        "width": width,
        "height": height,
        "layers": [{
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
        }]
    }))
    .expect("document metadata serializes")
}

fn load_archive(
    directory: &TestDir,
    name: &str,
    bytes: Vec<u8>,
) -> Result<document::LoadedDocument, ServiceError> {
    let cache = directory.path().join("cache");
    fs::create_dir_all(&cache).expect("cache directory is created");
    let path = directory.path().join(name);
    fs::write(&path, bytes).expect("archive is written");
    let file_io = LocalFileIo::new(&cache, directory.path().join("approvals.json"))
        .expect("file service opens");
    file_io.approve_path(&path).expect("archive is approved");
    document::load_document_candidate(&file_io, &path)
}

#[test]
fn limits_reject_invalid_values_and_preserve_budget_at_boundaries() {
    assert!(ptz_limits::validate_layer_id(&"a".repeat(128)).is_ok());
    for id in ["", "../escape", "a/b", ".", "..", "é"] {
        assert!(matches!(
            ptz_limits::validate_layer_id(id),
            Err(ServiceError::Validation(_))
        ));
    }
    assert!(matches!(
        ptz_limits::validate_layer_id(&"a".repeat(129)),
        Err(ServiceError::Validation(_))
    ));

    assert_eq!(
        ptz_limits::decoded_rgba_bytes(MAX_LAYER_DIMENSION, MAX_LAYER_DIMENSION).unwrap(),
        MAX_DOCUMENT_DECODED_RGBA_BYTES
    );
    for (width, height) in [(0, 1), (1, 0), (MAX_LAYER_DIMENSION + 1, 1)] {
        assert!(matches!(
            ptz_limits::decoded_rgba_bytes(width, height),
            Err(ServiceError::Validation(_))
        ));
    }
    assert!(matches!(
        ptz_limits::decoded_rgba_bytes(u64::MAX, 1),
        Err(ServiceError::ResourceLimit(_))
    ));

    let mut metadata = ArchiveBudget::new();
    assert!(matches!(
        metadata.reserve_document_json(MAX_DOCUMENT_JSON_BYTES + 1),
        Err(ServiceError::ResourceLimit(_))
    ));
    assert_eq!(metadata.entry_count(), 0);
    assert_eq!(metadata.total_encoded_bytes(), 0);
    metadata
        .reserve_document_json(MAX_DOCUMENT_JSON_BYTES)
        .expect("metadata byte limit is inclusive");
    assert_eq!(metadata.entry_count(), 1);

    let mut encoded = ArchiveBudget::new();
    encoded.reserve_document_json(7).unwrap();
    assert!(matches!(
        encoded.reserve_layer("over", 1, 1, MAX_ARCHIVE_ENTRY_BYTES - 6),
        Err(ServiceError::ResourceLimit(_))
    ));
    assert_eq!(encoded.layer_count(), 0);
    assert_eq!(encoded.entry_count(), 1);
    encoded
        .reserve_layer("at-limit", 1, 1, MAX_ARCHIVE_ENTRY_BYTES - 7)
        .expect("aggregate archive byte limit is inclusive");
    assert_eq!(encoded.total_encoded_bytes(), MAX_ARCHIVE_ENTRY_BYTES);
    assert!(matches!(
        encoded.reserve_layer("overflow", 1, 1, 1),
        Err(ServiceError::ResourceLimit(_))
    ));
    assert_eq!(encoded.layer_count(), 1);
    assert_eq!(encoded.entry_count(), 2);

    let mut decoded = ArchiveBudget::new();
    decoded.reserve_document_json(0).unwrap();
    decoded
        .reserve_layer("full", MAX_LAYER_DIMENSION, MAX_LAYER_DIMENSION, 0)
        .expect("decoded byte limit is inclusive");
    assert!(matches!(
        decoded.reserve_layer("overflow", 1, 1, 0),
        Err(ServiceError::ResourceLimit(_))
    ));
    assert_eq!(decoded.layer_count(), 1);
    assert_eq!(
        decoded.total_decoded_bytes(),
        MAX_DOCUMENT_DECODED_RGBA_BYTES
    );

    let mut layers = ArchiveBudget::new();
    layers.reserve_document_json(0).unwrap();
    for index in 0..MAX_LAYER_COUNT {
        layers
            .reserve_layer(&format!("layer-{index}"), 1, 1, 0)
            .unwrap();
    }
    assert_eq!(layers.layer_count(), MAX_LAYER_COUNT);
    assert_eq!(layers.entry_count(), MAX_STRUCTURAL_ENTRIES);
    assert!(matches!(
        layers.reserve_layer("overflow", 1, 1, 0),
        Err(ServiceError::ResourceLimit(_))
    ));
    assert_eq!(layers.layer_count(), MAX_LAYER_COUNT);
    assert_eq!(layers.entry_count(), MAX_STRUCTURAL_ENTRIES);
}

#[test]
fn file_io_rejects_unapproved_reads_without_persisting_approval() {
    let directory = TestDir::new("file-io");
    let cache = directory.path().join("cache");
    fs::create_dir_all(&cache).expect("cache directory is created");
    let approval_file = directory.path().join("approvals.json");
    let outside = directory.path().join("outside.ptz");
    fs::write(&outside, b"data").expect("unapproved file is written");
    let file_io = LocalFileIo::new(&cache, &approval_file).expect("file service opens");

    assert_error_code(
        file_io.approve_path(Path::new("invalid\0path")),
        "E_VALIDATION",
    );
    assert_error_code(file_io.read_approved(&outside, 4), "E_VALIDATION");
    assert_error_code(file_io.authorize_save_target(&outside), "E_VALIDATION");
    assert!(!approval_file.exists());

    file_io
        .approve_path(&outside)
        .expect("explicit approval persists");
    assert_error_code(file_io.read_approved(&outside, 3), "E_RESOURCE_LIMIT");
    assert_eq!(file_io.read_approved(&outside, 4).unwrap(), b"data");
}

#[test]
fn document_loader_and_core_parser_reject_bad_metadata_and_png() {
    assert!(PtzDocumentContent::from_ptz_document_json("{").is_err());
    let mut rgba = [0xA5; 4];
    assert!(photrez_core::pixel::export::decode_png_into(b"not a PNG", &mut rgba).is_err());
    assert_eq!(rgba, [0xA5; 4]);
    assert_error_code(
        load_archive(
            &TestDir::new("bad-metadata"),
            "bad-metadata.ptz",
            archive(&[("document.json", b"{")]),
        ),
        "E_VALIDATION",
    );

    let directory = TestDir::new("bad-png");
    let metadata = raster_document_json("layer-1", 1, 1);
    assert_error_code(
        load_archive(
            &directory,
            "bad-png.ptz",
            archive(&[
                ("document.json", &metadata),
                ("layers/layer-1.png", b"not a PNG"),
            ]),
        ),
        "E_VALIDATION",
    );
}

#[test]
fn document_loader_rejects_layer_dimensions_and_budget_before_png_decode() {
    let invalid_png = b"not a PNG";
    let oversized_width =
        u32::try_from(MAX_LAYER_DIMENSION + 1).expect("test dimension fits in u32");
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&raster_document_json("layer-1", oversized_width, 1))
            .expect("document metadata parses");
    metadata["width"] = serde_json::json!(1);
    metadata["height"] = serde_json::json!(1);
    let metadata = serde_json::to_vec(&metadata).expect("document metadata serializes");
    let dimension_error = load_archive(
        &TestDir::new("oversized-layer-dimension"),
        "oversized-layer-dimension.ptz",
        archive(&[
            ("document.json", &metadata),
            ("layers/layer-1.png", invalid_png),
        ]),
    )
    .err()
    .expect("oversized layer dimensions are rejected");
    assert_eq!(dimension_error.code(), "E_VALIDATION");
    assert!(matches!(
        &dimension_error,
        ServiceError::Validation(message) if message == "invalid raster width"
    ));

    let maximum_dimension =
        u32::try_from(MAX_LAYER_DIMENSION).expect("maximum dimension fits in u32");
    let mut metadata: serde_json::Value = serde_json::from_slice(&raster_document_json(
        "layer-1",
        maximum_dimension,
        maximum_dimension,
    ))
    .expect("document metadata parses");
    let mut second_layer = metadata["layers"][0].clone();
    second_layer["id"] = serde_json::Value::String("layer-2".into());
    metadata["layers"]
        .as_array_mut()
        .expect("document layers are an array")
        .push(second_layer);
    let metadata = serde_json::to_vec(&metadata).expect("document metadata serializes");
    let budget_error = load_archive(
        &TestDir::new("decoded-layer-budget"),
        "decoded-layer-budget.ptz",
        archive(&[
            ("document.json", &metadata),
            ("layers/layer-1.png", invalid_png),
            ("layers/layer-2.png", invalid_png),
        ]),
    )
    .err()
    .expect("decoded layer budget is rejected");
    assert_eq!(budget_error.code(), "E_RESOURCE_LIMIT");
    assert!(matches!(
        &budget_error,
        ServiceError::ResourceLimit(message)
            if message == "per-document decoded RGBA budget exceeded"
    ));
}

#[test]
fn save_stream_rejects_bad_inputs_and_cancel_removes_temporary_archive() {
    let directory = TestDir::new("save-inputs");
    let final_path = directory.path().join("save.ptz");
    let stream = SaveStream;
    let oversized_metadata = vec![0; MAX_DOCUMENT_JSON_BYTES as usize + 1];
    assert_error_code(
        stream.begin(
            save_target(directory.path(), "save.ptz"),
            &oversized_metadata,
        ),
        "E_RESOURCE_LIMIT",
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);

    let handle = stream
        .begin(save_target(directory.path(), "save.ptz"), b"{}")
        .expect("save starts");
    let png = png_1x1();
    assert_error_code(
        stream.write_layer(handle, "../escape", &png),
        "E_VALIDATION",
    );
    assert_error_code(
        stream.write_layer(handle, "malformed", b"not a PNG"),
        "E_VALIDATION",
    );
    stream
        .write_layer(handle, "same", &png)
        .expect("first layer is written");
    assert_error_code(stream.write_layer(handle, "same", &png), "E_VALIDATION");
    assert!(!final_path.exists());

    stream.cancel(handle).expect("failed input session cancels");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    let retry = stream
        .begin(save_target(directory.path(), "save.ptz"), b"{}")
        .expect("cancel releases target");
    stream.cancel(retry).expect("retry cancels");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn save_stream_failed_finalize_is_cleaned_by_cancel() {
    let directory = TestDir::new("finalize-cleanup");
    let final_path = directory.path().join("save.ptz");
    let stream = SaveStream;
    let handle = stream
        .begin(save_target(directory.path(), "save.ptz"), b"{}")
        .expect("save starts");

    fs::create_dir(&final_path).expect("conflicting target appears after begin");
    assert_error_code(stream.end(handle), "E_VALIDATION");
    stream.cancel(handle).expect("cleanup-only session cancels");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);

    fs::remove_dir(&final_path).expect("conflicting target is removed");
    let retry = stream
        .begin(save_target(directory.path(), "save.ptz"), b"{}")
        .expect("cancel releases path lock");
    stream.cancel(retry).expect("retry cancels");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn invalid_persisted_settings_use_the_missing_value_default() {
    let directory = TestDir::new("settings");
    let path = directory.path().join("settings.json");
    let settings = FileSettings::at(&path);
    assert_eq!(settings.read("photrez.locale"), None);

    fs::write(&path, b"{ not json").expect("junk settings are written");
    assert_eq!(settings.read("photrez.locale"), None);
    fs::write(&path, br#"{"photrez.locale":7}"#).expect("wrong value type is written");
    assert_eq!(settings.read("photrez.locale"), None);
}

#[test]
fn file_io_reports_missing_approved_file_and_recovers() {
    let directory = TestDir::new("file-io-missing");
    let cache = directory.path().join("cache");
    fs::create_dir_all(&cache).expect("cache directory is created");
    let approval_file = directory.path().join("approvals.json");
    let path = directory.path().join("gone.ptz");
    fs::write(&path, b"data").expect("approved file is written");
    let file_io = LocalFileIo::new(&cache, &approval_file).expect("file service opens");
    file_io.approve_path(&path).expect("path is approved");
    fs::remove_file(&path).expect("approved file is removed");

    assert_error_code(file_io.read_approved(&path, 4), "E_IO");

    fs::write(&path, b"back").expect("approved file is recreated");
    assert_eq!(
        file_io
            .read_approved(&path, 4)
            .expect("approved file reads after recovery"),
        b"back"
    );
}

#[test]
fn malformed_or_relative_persisted_approvals_are_rejected() {
    let directory = TestDir::new("approvals-invalid");
    let cache = directory.path().join("cache");
    fs::create_dir_all(&cache).expect("cache directory is created");
    let approval_file = directory.path().join("approvals.json");

    fs::write(&approval_file, b"{ not json").expect("malformed approvals are written");
    assert_error_code(LocalFileIo::new(&cache, &approval_file), "E_VALIDATION");

    fs::write(&approval_file, br#"["relative/path.ptz"]"#).expect("relative approval is written");
    assert_error_code(LocalFileIo::new(&cache, &approval_file), "E_VALIDATION");
}

#[test]
fn save_stream_reports_unwritable_parent_and_retry_succeeds() {
    let directory = TestDir::new("save-unwritable");
    let stream = SaveStream;
    let target = save_target(directory.path(), "save.ptz");
    fs::remove_dir_all(directory.path()).expect("save directory is removed");

    assert_error_code(stream.begin(target, b"{}"), "E_IO");

    fs::create_dir_all(directory.path()).expect("save directory is recreated");
    let handle = stream
        .begin(save_target(directory.path(), "save.ptz"), b"{}")
        .expect("a later save starts after the I/O refusal");
    stream.cancel(handle).expect("retry save cancels");
    assert_eq!(
        fs::read_dir(directory.path()).unwrap().count(),
        0,
        "cancel must leave no temporary archive"
    );
}

#[test]
fn document_loader_rejects_png_dimensions_before_decode() {
    let metadata = raster_document_json("layer-1", 2, 2);
    let png = png_1x1();
    let error = load_archive(
        &TestDir::new("png-dimension-mismatch"),
        "png-dimension-mismatch.ptz",
        archive(&[("document.json", &metadata), ("layers/layer-1.png", &png)]),
    )
    .err()
    .expect("1x1 PNG with 2x2 metadata is rejected");

    assert_eq!(error.code(), "E_VALIDATION");
    assert!(
        matches!(
            &error,
            ServiceError::Validation(message)
                if message == "raster PNG dimensions do not match document metadata"
        ),
        "{error}"
    );
}
