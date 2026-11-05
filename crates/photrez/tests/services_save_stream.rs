// Integration-test setup helpers fail the same way `#[test]` bodies may:
// with expect/panic on precondition violations. clippy.toml's
// allow-unwrap-in-tests only recognises test functions, so the allowance is
// spelled out here, scoped to this test-only file.
#![allow(clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use photrez::render::canvas_bridge::CanvasImage;
use photrez::services::document::load_document_candidate;
use photrez::services::file_io::{FileIo, LocalFileIo};
use photrez::services::save_stream::SaveStream;
use zip::{CompressionMethod, ZipArchive};

const FIXTURE_ARCHIVE: &[u8] = include_bytes!("fixtures/bounded-raster.ptz");
const SAME_WRITER_GOLDEN: &[u8] = include_bytes!("fixtures/bounded-raster.save-stream.golden.ptz");
const DOCUMENT_JSON: &[u8] = include_bytes!("fixtures/bounded-raster.document.json");
const LAYER_PNG: &[u8] = include_bytes!("fixtures/bounded-raster.layer.png");
const EXPECTED_RGBA: &[u8] = include_bytes!("fixtures/bounded-raster.rgba");
const EXPECTED_ENTRY_NAMES: [&str; 2] = ["document.json", "layers/raster-1.png"];
static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new() -> Self {
        for _ in 0..32 {
            let id = NEXT_TEMP_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "photrez-services-save-stream-{}-{id}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                // Link-free spelling: the file boundary rejects a path behind a link.
                Ok(()) => return Self(fs::canonicalize(&path).unwrap_or(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create temp directory: {error}"),
            }
        }
        panic!("could not allocate a unique temp directory");
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn approved_fixture() -> (TempDirectory, LocalFileIo, PathBuf) {
    let directory = TempDirectory::new();
    let fixture_path = directory.path().join("bounded-raster.ptz");
    fs::write(&fixture_path, FIXTURE_ARCHIVE).expect("fixture is copied to the temp path");
    let file_io = LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
        .expect("local file boundary opens");
    file_io
        .approve_path(&fixture_path)
        .expect("fixture path is explicitly approved");
    (directory, file_io, fixture_path)
}

fn archive_entry(archive_bytes: &[u8], name: &str) -> Vec<u8> {
    let mut archive = ZipArchive::new(Cursor::new(archive_bytes)).expect("archive is a ZIP");
    let mut entry = archive.by_name(name).expect("archive entry exists");
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .expect("archive entry can be read");
    bytes
}

fn write_save_stream_archive(
    directory: &TempDirectory,
    file_io: &LocalFileIo,
    document_json: &[u8],
    layer_png: &[u8],
) -> Vec<u8> {
    let output_path = directory.path().join("saved.ptz");
    let target = file_io
        .authorize_save_target(&output_path)
        .expect("cache-root save target is authorized");
    let save_stream = SaveStream;
    let handle = save_stream
        .begin(target, document_json)
        .expect("save stream begins");
    save_stream
        .write_layer(handle, "raster-1", layer_png)
        .expect("fixture layer is written");
    save_stream.end(handle).expect("save stream finalizes");
    fs::read(output_path).expect("saved archive is readable")
}

fn assert_entry_order(archive_bytes: &[u8]) {
    let mut archive = ZipArchive::new(Cursor::new(archive_bytes)).expect("archive is a ZIP");
    assert_eq!(archive.len(), EXPECTED_ENTRY_NAMES.len());
    for (index, expected_name) in EXPECTED_ENTRY_NAMES.iter().enumerate() {
        let entry = archive
            .by_index(index)
            .expect("expected archive entry exists");
        assert_eq!(
            entry.name().expect("entry name is valid").as_ref(),
            *expected_name
        );
    }
}

fn assert_fixed_entry_metadata(archive_bytes: &[u8]) {
    let mut archive = ZipArchive::new(Cursor::new(archive_bytes)).expect("archive is a ZIP");
    assert_eq!(archive.len(), EXPECTED_ENTRY_NAMES.len());
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .expect("expected archive entry exists");
        assert_eq!(entry.compression(), CompressionMethod::Deflated);
        assert_eq!(entry.last_modified(), Some(zip::DateTime::DEFAULT));
        assert_eq!(entry.system(), zip::System::Dos);
        assert_eq!(entry.external_attributes(), 0o600 << 16);
        assert!(!entry.encrypted());
        assert_eq!(entry.comment(), "");
        assert_eq!(entry.extra_data_fields().count(), 0);
    }
}

#[test]
fn approved_fixture_loads_to_canonical_rgba_accepted_by_canvas_image() {
    let (_directory, file_io, fixture_path) = approved_fixture();
    let loaded = load_document_candidate(&file_io, &fixture_path)
        .expect("production loader accepts fixture");

    assert_eq!(loaded.document_id(), "bounded-raster-doc");
    let content = loaded
        .content_snapshot()
        .expect("loaded document has typed content");
    assert_eq!(content.id, "bounded-raster-doc");
    assert_eq!((content.width, content.height), (2.0, 1.0));
    assert_eq!(content.layers.len(), 1);
    assert_eq!(content.layers[0].id, "raster-1");

    let snapshot = loaded.document_snapshot();
    assert_eq!(snapshot.id, "bounded-raster-doc");
    assert_eq!(snapshot.active_layer_id, None);
    assert_eq!(snapshot.viewport.zoom, 1.0);
    assert!(!snapshot.dirty);

    let (width, height, rgba) = loaded.canonical_rgba();
    assert_eq!((width, height), (2, 1));
    assert_eq!(rgba, EXPECTED_RGBA);
    let _image = CanvasImage::from_rgba(width, height, rgba.to_vec())
        .expect("display seam accepts the exact verified RGBA");
}

#[test]
fn save_stream_output_matches_same_writer_golden() {
    let (directory, file_io, _) = approved_fixture();
    let actual = write_save_stream_archive(&directory, &file_io, DOCUMENT_JSON, LAYER_PNG);
    assert_eq!(actual.as_slice(), SAME_WRITER_GOLDEN);
}

#[test]
fn save_stream_output_matches_independent_fixture_semantics() {
    let (directory, file_io, _) = approved_fixture();
    let document = archive_entry(FIXTURE_ARCHIVE, "document.json");
    let layer = archive_entry(FIXTURE_ARCHIVE, "layers/raster-1.png");
    assert_eq!(document, DOCUMENT_JSON);
    assert_eq!(layer, LAYER_PNG);

    let actual = write_save_stream_archive(&directory, &file_io, DOCUMENT_JSON, LAYER_PNG);
    assert_entry_order(FIXTURE_ARCHIVE);
    assert_entry_order(&actual);
    assert_fixed_entry_metadata(FIXTURE_ARCHIVE);
    assert_fixed_entry_metadata(&actual);
    assert_eq!(archive_entry(&actual, "document.json"), document);
    assert_eq!(archive_entry(&actual, "layers/raster-1.png"), layer);
}
