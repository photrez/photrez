use super::*;
use crate::services::error::ServiceError;
use crate::services::file_io::{FileIo, LocalFileIo};
use photrez_core::pixel::export::encode_image;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(1);

struct TestDir(PathBuf);

impl TestDir {
    fn new(label: &str) -> Self {
        let id = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "photrez-save-stream-{label}-{}-{id}",
            std::process::id()
        ));
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

fn target(directory: &Path, name: &str) -> SaveTarget {
    LocalFileIo::new(directory, directory.join("approvals.json"))
        .expect("save target authorizer opens")
        .authorize_save_target(&directory.join(name))
        .expect("cache target is authorized")
}

fn png() -> Vec<u8> {
    encode_image(1, 1, &[255, 0, 0, 255], "png", 100).expect("1x1 PNG encodes")
}

// Captured from the previous renderer: a 1x1 bitmap drawn through its own
// canvas and encoded as an "image/png" blob.
const REFERENCE_CANVAS_PNG: [u8; 88] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 1, 98, 58, 81, 161, 241, 31, 0, 0, 0,
    255, 255, 195, 173, 177, 143, 0, 0, 0, 6, 73, 68, 65, 84, 3, 0, 5, 230, 2, 106, 179, 130, 221,
    138, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

fn assert_code(error: ServiceError, expected: &str) {
    assert_eq!(error.code(), expected, "{error}");
}

fn read_entry(archive: &mut zip::ZipArchive<File>, name: &str) -> Vec<u8> {
    let mut entry = archive
        .by_name(name)
        .expect("expected archive entry exists");
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).expect("entry can be read");
    bytes
}

fn fail_next_finalize_sync(handle: SaveHandle) {
    registry()
        .sessions
        .get_mut(&handle)
        .expect("save session exists")
        .fail_finalize_sync = true;
}

fn fail_next_temp_removal(handle: SaveHandle) {
    registry()
        .sessions
        .get_mut(&handle)
        .expect("save session exists")
        .fail_remove_attempts = 1;
}

#[test]
fn approved_target_roundtrips_document_and_layer_bytes() {
    let directory = TestDir::new("roundtrip");
    let final_path = directory.path().join("roundtrip.ptz");
    let document = br#"{"format":1}"#;
    let layer = png();
    let stream = SaveStream;
    let handle = stream
        .begin(target(directory.path(), "roundtrip.ptz"), document)
        .expect("approved target starts");
    stream
        .write_layer(handle, "paint", &layer)
        .expect("layer is written");
    stream.end(handle).expect("archive commits");

    let mut archive = zip::ZipArchive::new(File::open(final_path).expect("target exists"))
        .expect("target is a ZIP archive");
    assert_eq!(archive.len(), 2);
    assert_eq!(read_entry(&mut archive, "document.json"), document);
    assert_eq!(read_entry(&mut archive, "layers/paint.png"), layer);
}

#[test]
fn reference_png_bytes_roundtrip_exactly_and_decode_to_one_pixel() {
    let directory = TestDir::new("reference-png");
    let final_path = directory.path().join("reference-png.ptz");
    let stream = SaveStream;
    let handle = stream
        .begin(target(directory.path(), "reference-png.ptz"), b"{}")
        .expect("save starts");
    stream
        .write_layer(handle, "reference", &REFERENCE_CANVAS_PNG)
        .expect("reference PNG writes");
    stream.end(handle).expect("archive commits");

    let mut archive = zip::ZipArchive::new(File::open(final_path).expect("target exists"))
        .expect("target is a ZIP archive");
    let saved_png = read_entry(&mut archive, "layers/reference.png");
    assert_eq!(saved_png, REFERENCE_CANVAS_PNG);

    let mut rgba = [0_u8; 4];
    let dimensions = photrez_core::pixel::export::decode_png_into(&saved_png, &mut rgba)
        .expect("saved reference PNG decodes");
    assert_eq!((dimensions.width(), dimensions.height()), (1, 1));
    assert_eq!(
        rgba.len(),
        dimensions.width() as usize * dimensions.height() as usize * 4
    );
}

#[test]
fn invalid_layer_id_is_rejected_before_zip_mutation() {
    let directory = TestDir::new("invalid-id");
    let final_path = directory.path().join("invalid-id.ptz");
    let stream = SaveStream;
    let handle = stream
        .begin(target(directory.path(), "invalid-id.ptz"), b"{}")
        .expect("save starts");
    assert_code(
        stream
            .write_layer(handle, "../escape", &png())
            .expect_err("invalid layer ID is rejected"),
        "E_VALIDATION",
    );
    stream
        .write_layer(handle, "valid", &png())
        .expect("session remains usable after validation failure");
    stream.end(handle).expect("archive commits");

    let mut archive = zip::ZipArchive::new(File::open(final_path).expect("target exists"))
        .expect("target is a ZIP archive");
    assert_eq!(archive.len(), 2);
    assert_eq!(read_entry(&mut archive, "layers/valid.png"), png());
}

#[test]
fn duplicate_layer_id_is_rejected_before_zip_mutation() {
    let directory = TestDir::new("duplicate-id");
    let final_path = directory.path().join("duplicate-id.ptz");
    let layer = png();
    let stream = SaveStream;
    let handle = stream
        .begin(target(directory.path(), "duplicate-id.ptz"), b"{}")
        .expect("save starts");
    stream
        .write_layer(handle, "same", &layer)
        .expect("first layer is written");
    assert_code(
        stream
            .write_layer(handle, "same", &layer)
            .expect_err("duplicate layer ID is rejected"),
        "E_VALIDATION",
    );
    stream.end(handle).expect("archive commits");

    let mut archive = zip::ZipArchive::new(File::open(final_path).expect("target exists"))
        .expect("target is a ZIP archive");
    assert_eq!(archive.len(), 2);
    assert_eq!(read_entry(&mut archive, "layers/same.png"), layer);
}

#[test]
fn same_path_conflicts_and_path_lock_releases_after_cancel_and_end() {
    let directory = TestDir::new("path-lock");
    let final_path = directory.path().join("path-lock.ptz");
    let stream = SaveStream;
    let first = stream
        .begin(target(directory.path(), "path-lock.ptz"), b"{}")
        .expect("first save starts");
    assert_code(
        stream
            .begin(target(directory.path(), "path-lock.ptz"), b"{}")
            .expect_err("same final path cannot have two active sessions"),
        "E_CONFLICT",
    );
    stream.cancel(first).expect("first save cancels");

    let second = stream
        .begin(target(directory.path(), "path-lock.ptz"), b"{}")
        .expect("cancel releases path lock");
    stream.end(second).expect("second save commits");
    fs::remove_file(&final_path).expect("committed target is removed for lock check");
    let third = stream
        .begin(target(directory.path(), "path-lock.ptz"), b"{}")
        .expect("end releases path lock");
    stream.end(third).expect("third save commits");
}

#[test]
fn metadata_limit_rejects_before_creating_final_or_temporary_file() {
    let directory = TestDir::new("metadata-limit");
    let final_path = directory.path().join("metadata-limit.ptz");
    let document = vec![b'x'; crate::services::ptz_limits::MAX_DOCUMENT_JSON_BYTES as usize + 1];
    assert_code(
        SaveStream
            .begin(target(directory.path(), "metadata-limit.ptz"), &document)
            .expect_err("oversized document metadata is rejected"),
        "E_RESOURCE_LIMIT",
    );
    assert!(!final_path.exists());
    assert_eq!(
        fs::read_dir(directory.path())
            .expect("directory reads")
            .count(),
        0
    );
}

#[test]
fn layer_resource_limit_rejection_leaves_final_absent() {
    let directory = TestDir::new("layer-limit");
    let final_path = directory.path().join("layer-limit.ptz");
    let stream = SaveStream;
    let handle = stream
        .begin(target(directory.path(), "layer-limit.ptz"), b"{}")
        .expect("save starts");
    // Reserve the encoded budget without allocating or writing a GiB-sized buffer.
    super::registry()
        .sessions
        .get_mut(&handle)
        .expect("active session exists")
        .budget
        .reserve_layer(
            "reserved",
            1,
            1,
            crate::services::ptz_limits::MAX_ARCHIVE_ENTRY_BYTES - 2,
        )
        .expect("two-byte document plus reservation fills the encoded budget");
    assert_code(
        stream
            .write_layer(handle, "large", &png())
            .expect_err("encoded PNG exceeds the remaining archive budget"),
        "E_RESOURCE_LIMIT",
    );
    stream.cancel(handle).expect("rejected save cancels");
    assert!(!final_path.exists());
}

#[test]
fn cancel_removes_temporary_file_and_leaves_final_untouched() {
    let directory = TestDir::new("cancel");
    let final_path = directory.path().join("cancel.ptz");
    let stream = SaveStream;
    let handle = stream
        .begin(target(directory.path(), "cancel.ptz"), b"{}")
        .expect("save starts");
    assert!(!final_path.exists());
    assert_eq!(
        fs::read_dir(directory.path())
            .expect("directory reads")
            .count(),
        1
    );
    stream.cancel(handle).expect("save cancels");
    assert!(!final_path.exists());
    assert_eq!(
        fs::read_dir(directory.path())
            .expect("directory reads")
            .count(),
        0
    );
}

#[test]
fn end_atomically_commits_only_after_archive_is_finished() {
    let directory = TestDir::new("atomic-end");
    let final_path = directory.path().join("atomic-end.ptz");
    let stream = SaveStream;
    let handle = stream
        .begin(target(directory.path(), "atomic-end.ptz"), b"{}")
        .expect("save starts");
    assert!(!final_path.exists());
    stream.end(handle).expect("archive commits");
    assert!(final_path.exists());
    assert_eq!(
        fs::read_dir(directory.path())
            .expect("directory reads")
            .count(),
        1
    );
    let archive = zip::ZipArchive::new(File::open(final_path).expect("target exists"))
        .expect("committed target is a ZIP archive");
    assert_eq!(archive.len(), 1);
}

#[test]
fn existing_target_is_replaced_only_at_successful_finalize() {
    let directory = TestDir::new("replace-existing");
    let final_path = directory.path().join("existing.ptz");
    let original = b"previous committed bytes";
    fs::write(&final_path, original).expect("existing target is created");
    let document = br#"{"replacement":true}"#;
    let stream = SaveStream;
    let handle = stream
        .begin(target(directory.path(), "existing.ptz"), document)
        .expect("existing approved regular target starts");
    assert_eq!(
        fs::read(&final_path).expect("old target remains readable"),
        original
    );
    stream
        .write_layer(handle, "paint", &png())
        .expect("replacement layer writes");
    assert_eq!(
        fs::read(&final_path).expect("old target remains readable"),
        original
    );
    stream.end(handle).expect("replacement commits");

    let mut archive = zip::ZipArchive::new(File::open(final_path).expect("new target exists"))
        .expect("new target is a ZIP archive");
    assert_eq!(read_entry(&mut archive, "document.json"), document);
}

#[test]
fn cancel_preserves_existing_target_bytes() {
    let directory = TestDir::new("cancel-existing");
    let final_path = directory.path().join("existing.ptz");
    let original = b"existing target stays unchanged";
    fs::write(&final_path, original).expect("existing target is created");
    let stream = SaveStream;
    let handle = stream
        .begin(target(directory.path(), "existing.ptz"), b"{}")
        .expect("existing target starts");
    assert_eq!(
        fs::read(&final_path).expect("target remains readable"),
        original
    );
    stream.cancel(handle).expect("save cancels");
    assert_eq!(
        fs::read(&final_path).expect("target remains readable"),
        original
    );
}

#[test]
fn finalize_failure_retains_handle_for_cancel() {
    let directory = TestDir::new("finalize-failure");
    let final_path = directory.path().join("failure.ptz");
    let stream = SaveStream;
    let handle = stream
        .begin(target(directory.path(), "failure.ptz"), b"{}")
        .expect("save starts");
    fail_next_finalize_sync(handle);
    assert_code(
        stream
            .end(handle)
            .expect_err("injected sync failure is returned"),
        "E_IO",
    );
    assert!(!final_path.exists());
    assert_code(
        stream
            .begin(target(directory.path(), "failure.ptz"), b"{}")
            .expect_err("path remains locked while cleanup is pending"),
        "E_CONFLICT",
    );
    stream
        .cancel(handle)
        .expect("retained handle cancels cleanup-only session");
    assert!(!final_path.exists());
    let next = stream
        .begin(target(directory.path(), "failure.ptz"), b"{}")
        .expect("cancel releases path lock");
    stream.cancel(next).expect("new session cancels");
}

#[test]
fn cancel_retries_temp_cleanup_using_the_same_handle() {
    let directory = TestDir::new("cleanup-retry");
    let final_path = directory.path().join("retry.ptz");
    let stream = SaveStream;
    let handle = stream
        .begin(target(directory.path(), "retry.ptz"), b"{}")
        .expect("save starts");
    fail_next_temp_removal(handle);
    assert_code(
        stream
            .cancel(handle)
            .expect_err("injected temp removal error is returned"),
        "E_IO",
    );
    assert_code(
        stream
            .begin(target(directory.path(), "retry.ptz"), b"{}")
            .expect_err("path stays locked while temp cleanup is uncertain"),
        "E_CONFLICT",
    );
    assert!(!final_path.exists());
    stream
        .cancel(handle)
        .expect("same handle retries and completes temp cleanup");
    assert!(!final_path.exists());
    let next = stream
        .begin(target(directory.path(), "retry.ptz"), b"{}")
        .expect("successful retry releases path lock");
    stream.cancel(next).expect("new session cancels");
}

#[test]
fn symlink_substitution_is_rejected_without_touching_its_target() {
    let directory = TestDir::new("symlink-substitution");
    let final_path = directory.path().join("target.ptz");
    let victim_path = directory.path().join("victim.ptz");
    let victim_bytes = b"symlink target bytes";
    fs::write(&victim_path, victim_bytes).expect("victim file is created");
    let stream = SaveStream;
    let handle = stream
        .begin(target(directory.path(), "target.ptz"), b"{}")
        .expect("save starts before substitution");

    #[cfg(windows)]
    let link_result = std::os::windows::fs::symlink_file(&victim_path, &final_path);
    #[cfg(unix)]
    let link_result = std::os::unix::fs::symlink(&victim_path, &final_path);
    #[cfg(not(any(windows, unix)))]
    let link_result: std::io::Result<()> = Err(std::io::Error::other(
        "symlink unsupported on this platform",
    ));
    if let Err(error) = link_result {
        eprintln!("SKIP symlink substitution: host could not create a file symlink: {error}");
        stream
            .cancel(handle)
            .expect("save cancels after symlink skip");
        return;
    }

    assert_code(
        stream
            .end(handle)
            .expect_err("final target symlink substitution is rejected"),
        "E_VALIDATION",
    );
    assert!(fs::symlink_metadata(&final_path)
        .expect("substitution remains a symlink")
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read(&victim_path).expect("victim remains readable"),
        victim_bytes
    );
    stream
        .cancel(handle)
        .expect("failed finalize session remains cancellable");
}
