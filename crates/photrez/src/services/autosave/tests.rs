use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::services::autosave::{AutosaveManifest, AutosaveStore, SaveJob, SaveQueue};
use crate::services::document::load_document_candidate;
use crate::services::file_io::{FileIo, LocalFileIo};
use crate::services::save_stream::SaveStream;
use crate::services::session::SessionRegistry;

static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);
const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/bounded-raster.ptz");

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        for _ in 0..32 {
            let id = NEXT_TEMP_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("photrez-autosave-{}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                // Hand the file boundary the link-free spelling of the scratch
                // root; a temp directory can sit behind a symbolic link.
                Ok(()) => return Self(fs::canonicalize(&path).unwrap_or(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create test directory: {error}"),
            }
        }
        panic!("could not allocate a unique test directory");
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn read_ids(store: &AutosaveStore) -> Vec<String> {
    store
        .list()
        .expect("manifest lists")
        .into_iter()
        .map(|entry| entry.doc_id)
        .collect()
}

fn dirty_fixture(directory: &TestDirectory) -> (LocalFileIo, SessionRegistry, PathBuf) {
    let files = LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
        .expect("file service opens");
    let source_path = directory.path().join("source.ptz");
    fs::write(&source_path, FIXTURE).expect("fixture is copied");
    files
        .approve_path(&source_path)
        .expect("fixture is approved");
    let loaded = load_document_candidate(&files, &source_path).expect("fixture loads");
    let mut sessions = SessionRegistry::default();
    sessions
        .open(loaded, "Fixture", Some(source_path.clone()))
        .expect("session opens");
    sessions.mark_dirty("parity-doc").expect("session is dirty");
    (files, sessions, source_path)
}

#[test]
fn manifest_round_trip_is_deterministic_and_accepts_legacy_values() {
    let manifest = AutosaveManifest::parse(br#"{"z":"Last|12","a":"Legacy name"}"#)
        .expect("legacy value is accepted");
    let encoded = manifest.encode().expect("manifest serializes");
    assert_eq!(encoded, br#"{"a":"Legacy name","z":"Last|12"}"#);
    assert_eq!(
        AutosaveManifest::parse(&encoded)
            .expect("encoded manifest parses")
            .encode()
            .unwrap(),
        encoded
    );
}

#[test]
fn malformed_or_unsafe_manifest_is_rejected_without_mutation() {
    let directory = TestDirectory::new();
    let store = AutosaveStore::new(directory.path());
    fs::create_dir_all(store.autosave_dir()).expect("autosave directory is created");
    let archive_path = store.archive_path("safe-doc").expect("safe ID accepted");
    fs::write(&archive_path, b"preserve archive").expect("archive sentinel is written");
    let manifest_path = store.manifest_path();
    fs::write(manifest_path, b"{").expect("malformed manifest is written");

    assert_eq!(store.list().unwrap_err().code(), "E_VALIDATION");
    assert_eq!(store.clear_all().unwrap_err().code(), "E_VALIDATION");
    assert_eq!(fs::read(manifest_path).unwrap(), b"{");
    assert_eq!(fs::read(&archive_path).unwrap(), b"preserve archive");

    fs::write(manifest_path, br#"{"../escape":"Unsafe|1"}"#).expect("unsafe manifest is written");
    assert_eq!(store.list().unwrap_err().code(), "E_VALIDATION");
    assert_eq!(
        store.clear_one("../escape").unwrap_err().code(),
        "E_VALIDATION"
    );
    assert_eq!(
        fs::read(manifest_path).unwrap(),
        br#"{"../escape":"Unsafe|1"}"#
    );

    fs::write(manifest_path, br#"{"safe-doc":"Name|not-a-timestamp"}"#)
        .expect("malformed timestamp is written");
    assert_eq!(store.list().unwrap_err().code(), "E_VALIDATION");
    assert_eq!(fs::read(&archive_path).unwrap(), b"preserve archive");
}

#[test]
fn stale_cleanup_retains_failed_deletions_and_unrelated_entries() {
    let directory = TestDirectory::new();
    let store = AutosaveStore::new(directory.path());
    fs::create_dir_all(store.autosave_dir()).expect("autosave directory is created");
    let now = 10_000_000_000;
    let stale = now - super::STALE_AGE_MS - 1;
    let fresh = now - super::STALE_AGE_MS + 1;
    let manifest = AutosaveManifest::parse(
        format!(r#"{{"good":"Good|{stale}","failed":"Failed|{stale}","fresh":"Fresh|{fresh}"}}"#)
            .as_bytes(),
    )
    .expect("manifest parses");
    store.write_manifest(&manifest).expect("manifest writes");

    let good_path = store.archive_path("good").unwrap();
    fs::write(&good_path, b"stale").expect("stale archive is written");
    let failed_path = store.archive_path("failed").unwrap();
    fs::create_dir(&failed_path).expect("failed deletion target is a directory");
    fs::write(failed_path.join("child"), b"keep").expect("directory is non-empty");
    let fresh_path = store.archive_path("fresh").unwrap();
    fs::write(&fresh_path, b"fresh").expect("fresh archive is written");

    assert!(store.cleanup_stale(now).is_err());
    assert!(!good_path.exists());
    assert!(failed_path.is_dir());
    assert_eq!(fs::read(&fresh_path).unwrap(), b"fresh");
    assert_eq!(read_ids(&store), ["failed", "fresh"]);
}

#[test]
fn manifest_replacement_is_atomic_and_clear_operations_are_explicit() {
    let directory = TestDirectory::new();
    let store = AutosaveStore::new(directory.path());
    fs::create_dir_all(store.autosave_dir()).expect("autosave directory is created");
    let manifest_path = store.manifest_path();
    fs::write(manifest_path, br#"{"old":"Old|1"}"#).expect("prior manifest is written");
    let manifest =
        AutosaveManifest::parse(br#"{"z":"Zed|2","a":"A"}"#).expect("replacement manifest parses");
    store
        .write_manifest(&manifest)
        .expect("manifest is replaced");
    assert_eq!(
        fs::read(manifest_path).unwrap(),
        br#"{"a":"A","z":"Zed|2"}"#
    );
    assert_eq!(fs::read_dir(store.autosave_dir()).unwrap().count(), 1);

    for id in ["a", "z"] {
        fs::write(store.archive_path(id).unwrap(), id).expect("archive is written");
    }
    assert!(store.clear_one("a").expect("one entry clears"));
    assert!(!store.archive_path("a").unwrap().exists());
    assert_eq!(read_ids(&store), ["z"]);
    assert_eq!(store.clear_all().expect("all entries clear"), 1);
    assert!(read_ids(&store).is_empty());
}

#[test]
fn failed_manifest_replacement_preserves_previous_bytes_and_cleans_temp_file() {
    let directory = TestDirectory::new();
    let store = AutosaveStore::new(directory.path());
    let old = AutosaveManifest::parse(br#"{"old":"Old|1"}"#).unwrap();
    store.write_manifest(&old).expect("prior manifest writes");
    let prior = fs::read(store.manifest_path()).expect("prior manifest reads");
    let replacement = AutosaveManifest::parse(br#"{"new":"New|2"}"#).unwrap();

    store.fail_next_manifest_rename();
    assert_eq!(
        store.write_manifest(&replacement).unwrap_err().code(),
        "E_IO"
    );
    assert_eq!(fs::read(store.manifest_path()).unwrap(), prior);
    assert_eq!(fs::read_dir(store.autosave_dir()).unwrap().count(), 1);
}

#[test]
fn queue_rejects_low_priority_work_and_allows_one_manual_pending_job() {
    let mut queue = SaveQueue::default();
    assert!(queue.enqueue_autosave("auto"));
    assert!(queue.enqueue_manual("manual"));
    assert!(!queue.enqueue_manual("manual-2"));
    assert_eq!(queue.pending(), Some(&SaveJob::Manual("manual".into())));
    assert!(!queue.enqueue_autosave("later"));
    assert_eq!(queue.complete(), Some(SaveJob::Manual("manual".into())));
    assert!(!queue.enqueue_autosave("later"));
    assert_eq!(queue.complete(), None);
    assert!(queue.enqueue_autosave("later"));
}

#[test]
fn cache_paths_are_bounded_by_the_injected_root() {
    let directory = TestDirectory::new();
    let store = AutosaveStore::new(directory.path());
    let path = store.archive_path("safe-id").expect("safe ID accepted");
    assert!(path.starts_with(directory.path()));
    assert_eq!(path, directory.path().join("autosave").join("safe-id.ptz"));
    assert_eq!(
        store.archive_path("../escape").unwrap_err().code(),
        "E_VALIDATION"
    );
}

#[test]
fn recovery_path_uses_validated_entry_under_injected_cache_root() {
    let directory = TestDirectory::new();
    let store = AutosaveStore::new(directory.path());
    let manifest = AutosaveManifest::parse(br#"{"safe-id":"Safe|1"}"#).unwrap();
    store.write_manifest(&manifest).expect("manifest writes");
    let entry = store.list().unwrap().remove(0);

    let path = store.recovery_path(&entry).expect("recovery path resolves");
    assert_eq!(path, directory.path().join("autosave").join("safe-id.ptz"));
    assert!(path.starts_with(directory.path()));

    let unsafe_entry = super::AutosaveEntry {
        doc_id: "../escape".into(),
        display_name: "Unsafe".into(),
        unix_ms: None,
    };
    assert_eq!(
        store.recovery_path(&unsafe_entry).unwrap_err().code(),
        "E_VALIDATION"
    );
}

#[test]
fn dirty_loaded_fixture_autosaves_canonically_without_changing_session_state() {
    let directory = TestDirectory::new();
    let (files, sessions, source_path) = dirty_fixture(&directory);
    let store = AutosaveStore::new(directory.path());
    let session = sessions.get("parity-doc").unwrap();
    assert!(store
        .autosave("parity-doc", session, &files, &SaveStream, 1234)
        .expect("dirty document autosaves"));

    let entries = store.list().expect("manifest lists saved document");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].doc_id, "parity-doc");
    assert_eq!(entries[0].display_name, "Fixture");
    assert_eq!(entries[0].unix_ms, Some(1234));
    let recovery_path = store
        .recovery_path(&entries[0])
        .expect("recovery path resolves");
    assert!(recovery_path.is_file());
    let session = sessions.get("parity-doc").unwrap();
    assert_eq!(session.source_path(), Some(source_path.as_path()));
    assert!(session.is_dirty());

    files
        .approve_path(&recovery_path)
        .expect("cache archive is approved for read");
    let recovered = load_document_candidate(&files, &recovery_path).expect("autosave reloads");
    assert_eq!(recovered.document_id(), "parity-doc");
}

#[test]
fn autosave_rejects_document_id_mismatch_and_does_not_write_manifest_on_save_failure() {
    let directory = TestDirectory::new();
    let (files, sessions, _) = dirty_fixture(&directory);
    let store = AutosaveStore::new(directory.path());
    let session = sessions.get("parity-doc").unwrap();
    assert_eq!(
        store
            .autosave("other-id", session, &files, &SaveStream, 1)
            .unwrap_err()
            .code(),
        "E_VALIDATION"
    );
    assert!(!store.manifest_path().exists());
    assert!(read_ids(&store).is_empty());

    let other_directory = TestDirectory::new();
    let other_store = AutosaveStore::new(other_directory.path());
    assert_eq!(
        other_store
            .autosave("parity-doc", session, &files, &SaveStream, 2)
            .unwrap_err()
            .code(),
        "E_VALIDATION"
    );
    assert!(!other_store.manifest_path().exists());
    assert!(!other_store.archive_path("parity-doc").unwrap().exists());
    assert!(session.is_dirty());
}
