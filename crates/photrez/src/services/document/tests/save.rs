use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::services::file_io::{FileIo, LocalFileIo};
use crate::services::save_stream::SaveStream;
use photrez_core::pixel::export::encode_image;

use super::{archive, document_json, load, png_rgba, raster_layer, single_raster_archive, PNG_1X1};

static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(1);

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let id = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("photrez-document-save-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).expect("test directory is created");
        Self(path)
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

fn read_entry(path: &Path, name: &str) -> Vec<u8> {
    let file = File::open(path).expect("saved archive opens");
    let mut archive = zip::ZipArchive::new(file).expect("saved archive parses");
    let mut entry = archive.by_name(name).expect("archive entry exists");
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).expect("archive entry reads");
    bytes
}

fn save_path(directory: &Path) -> PathBuf {
    directory.join("saved.ptz")
}

fn file_io(directory: &Path) -> LocalFileIo {
    LocalFileIo::new(directory, directory.join("approvals.json")).expect("test file service opens")
}

#[test]
fn save_roundtrips_fixture_with_canonical_v3_metadata_and_png_bytes() {
    let loaded = load(single_raster_archive("layer-1", 1, 1, &PNG_1X1)).expect("fixture loads");
    let document_json = loaded
        .document_snapshot()
        .to_json()
        .expect("v3 document JSON serializes")
        .into_bytes();
    let metadata: serde_json::Value =
        serde_json::from_slice(&document_json).expect("v3 document JSON parses");
    assert_eq!(metadata["format"], "photrez-ptz");
    assert_eq!(metadata["version"], 3);
    let layer = loaded
        .registry
        .get_layer(loaded.document_id(), "layer-1")
        .expect("canonical pixels are present");
    let rgba = layer.snapshot_region(0, 0, 1, 1);
    let expected_png = encode_image(1, 1, &rgba, "png", 100).expect("canonical PNG encodes");

    let directory = TestDir::new();
    let path = save_path(directory.path());
    let files = file_io(directory.path());
    let stream = SaveStream;
    loaded
        .save_to(&files, &path, &stream)
        .expect("loaded fixture saves");

    assert_eq!(read_entry(&path, "document.json"), document_json);
    assert_eq!(read_entry(&path, "layers/layer-1.png"), expected_png);

    let saved = load(fs::read(&path).expect("saved bytes read")).expect("saved fixture reloads");
    assert_eq!(saved.document_snapshot(), loaded.document_snapshot());
    assert_eq!(saved.canonical_rgba().2, rgba);
}

#[test]
fn multiple_raster_layers_save_and_reload_with_the_same_composite() {
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
    .expect("two raster layers load");
    let document_json = loaded
        .document_snapshot()
        .to_json()
        .expect("v3 document JSON serializes")
        .into_bytes();

    let directory = TestDir::new();
    let path = save_path(directory.path());
    let files = file_io(directory.path());
    loaded
        .save_to(&files, &path, &SaveStream)
        .expect("multi-layer document saves");

    assert_eq!(read_entry(&path, "document.json"), document_json);
    assert_eq!(read_entry(&path, "layers/layer-top.png"), top);
    assert_eq!(read_entry(&path, "layers/layer-bottom.png"), bottom);

    let saved = load(fs::read(&path).expect("saved bytes read")).expect("saved fixture reloads");
    assert_eq!(saved.document_snapshot(), loaded.document_snapshot());
    assert_eq!(saved.canonical_rgba().2, loaded.canonical_rgba().2);
}

#[test]
fn save_writes_live_content_that_no_longer_matches_the_file() {
    let mut loaded = load(single_raster_archive("layer-1", 1, 1, &PNG_1X1)).expect("fixture loads");
    let mut content = loaded
        .content_snapshot()
        .expect("typed content is present")
        .clone();
    content.layers.push(content.layers[0].clone());
    let document_id = loaded.document_id().to_owned();
    let second_layer_id = "layer-2";
    loaded
        .registry
        .add_layer(&document_id, second_layer_id, 1, 1, vec![0; 4])
        .expect("second layer pixels seed");
    content.layers[1].id = second_layer_id.into();
    loaded
        .registry
        .seed_document(&document_id, content.clone())
        .expect("test content seeds");

    let directory = TestDir::new();
    let path = save_path(directory.path());
    let files = file_io(directory.path());
    let stream = SaveStream;

    loaded
        .save_to(&files, &path, &stream)
        .expect("live canonical content saves");

    let saved = load(fs::read(&path).expect("saved bytes read")).expect("saved fixture reloads");
    let saved_content = saved.content_snapshot().expect("saved content is present");
    assert_eq!(
        saved_content.layers.len(),
        2,
        "a layer added after load must be written"
    );
    assert_eq!(saved_content.layers[1].id, "layer-2");
    assert_eq!(saved_content.layers[1].name, content.layers[1].name);
    assert_eq!(saved_content.selection, content.selection);
    assert!(
        saved
            .registry
            .get_layer(saved.document_id(), second_layer_id)
            .is_some(),
        "the added layer's pixels must round-trip"
    );
}

#[test]
fn shape_layers_save_in_document_json_without_a_png_entry() {
    let background = png_rgba(8, 8, &[0, 0, 255, 255].repeat(64));
    let shape = super::typed_layer(
        "shape",
        photrez_core::ptz::content::LayerType::Shape,
        4.0,
        4.0,
        (1.0, 1.0),
        Some(photrez_core::ptz::content::ShapeParams {
            kind: photrez_core::ptz::content::ShapeKind::Rect,
            width: 4.0,
            height: 4.0,
            radius: 0.0,
            fill: photrez_core::ptz::content::ShapeFill {
                kind: photrez_core::ptz::content::ShapeFillKind::Solid,
                color: "#ff0000".to_owned(),
            },
            stroke: photrez_core::ptz::content::ShapeStroke {
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
    .expect("shape document loads");

    let directory = TestDir::new();
    let path = save_path(directory.path());
    let files = file_io(directory.path());
    loaded
        .save_to(&files, &path, &SaveStream)
        .expect("shape document saves");

    let file = File::open(&path).expect("saved archive opens");
    let mut archive = zip::ZipArchive::new(file).expect("saved archive parses");
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).expect("archive entry opens");
        entries.push(
            std::str::from_utf8(entry.name_raw())
                .expect("archive entry name is UTF-8")
                .to_owned(),
        );
    }
    entries.sort();
    assert_eq!(
        entries,
        [
            "document.json".to_owned(),
            "layers/background.png".to_owned()
        ]
    );

    let saved = load(fs::read(&path).expect("saved bytes read")).expect("saved fixture reloads");
    assert_eq!(saved.document_snapshot(), loaded.document_snapshot());
    assert_eq!(saved.canonical_rgba().2, loaded.canonical_rgba().2);
}

#[test]
fn save_stream_cancel_after_layer_failure_removes_temporary_archive() {
    let directory = TestDir::new();
    let path = save_path(directory.path());
    let prior = b"existing target remains untouched";
    fs::write(&path, prior).expect("prior target is created");
    let files = file_io(directory.path());
    let target = files
        .authorize_save_target(&path)
        .expect("target is authorized");
    let stream = SaveStream;
    let handle = stream
        .begin(target, b"{}")
        .expect("stream starts in a temporary archive");

    let error = stream
        .write_layer(handle, "layer-1", b"invalid PNG")
        .expect_err("invalid PNG is rejected");
    assert_eq!(error.code(), "E_VALIDATION", "{error}");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    stream
        .cancel(handle)
        .expect("cancel cleans the temporary archive");

    assert_eq!(fs::read(&path).expect("prior target remains"), prior);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

/// Save-seam check: a multi-layer document with a shape and a text layer must
/// save and reload to the same canonical composite and the same typed
/// parameters. The expected composite is the live first load, so this pins
/// the save/read seam rather than the renderer alone.
#[test]
fn multi_layer_shape_text_save_reload_preserves_composite_and_params() {
    use super::typed_layer;
    use photrez_core::ptz::content::LayerType;
    use photrez_core::ptz::layer_params::{
        ShapeFill, ShapeFillKind, ShapeKind, ShapeParams, ShapeStroke, TextData,
    };

    let background = png_rgba(8, 8, &[0, 0, 255, 255].repeat(64));
    let shape = typed_layer(
        "shape",
        LayerType::Shape,
        4.0,
        4.0,
        (1.0, 1.0),
        Some(ShapeParams {
            kind: ShapeKind::Ellipse,
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
    let text = typed_layer(
        "text",
        LayerType::Text,
        16.0,
        16.0,
        (0.0, 6.0),
        None,
        Some(TextData {
            content: "Ag".to_owned(),
            font_family: "Inter".to_owned(),
            font_size: 10.0,
            line_height: 1.2,
            color: "#00ff00".to_owned(),
            ..Default::default()
        }),
    );
    let metadata = document_json(vec![text, shape, raster_layer("background", 8, 8)], 8, 8);
    let bytes = archive(&[
        ("document.json", &metadata),
        ("layers/background.png", &background),
    ]);
    let loaded = load(bytes.clone()).expect("multi-layer fixture loads");
    let expected = loaded.canonical_rgba().2.to_vec();

    let directory = TestDir::new();
    let path = save_path(directory.path());
    let files = file_io(directory.path());
    loaded
        .save_to(&files, &path, &SaveStream)
        .expect("multi-layer document saves");

    let saved = fs::read(&path).expect("saved archive reads");
    let reloaded = load(saved).expect("saved multi-layer document reloads");
    assert_eq!(
        reloaded.canonical_rgba().2,
        expected.as_slice(),
        "save/reload must reproduce the composite byte for byte"
    );
    let content = reloaded.content_snapshot().expect("content exists");
    assert_eq!(content.layers.len(), 3);
    let shape_layer = content
        .layers
        .iter()
        .find(|layer| layer.id == "shape")
        .expect("shape layer survives");
    assert_eq!(
        shape_layer
            .shape_params
            .as_ref()
            .map(|params| params.kind.clone()),
        Some(ShapeKind::Ellipse)
    );
    let text_layer = content
        .layers
        .iter()
        .find(|layer| layer.id == "text")
        .expect("text layer survives");
    assert_eq!(
        text_layer
            .text_data
            .as_ref()
            .map(|data| data.content.as_str()),
        Some("Ag")
    );
}
