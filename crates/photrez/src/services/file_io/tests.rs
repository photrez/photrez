use super::{FileIo, LocalFileIo};
#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(1);

struct TestDir(PathBuf);

impl TestDir {
    fn new(label: &str) -> Self {
        let id = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "photrez-file-io-{label}-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("test directory is created");
        // Link-free spelling: the boundary under test rejects a path behind a link.
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

fn assert_code(error: super::super::error::ServiceError, expected: &str) {
    assert_eq!(error.code(), expected, "{error}");
}

/// The handle-bound open behind `read_approved` is implemented only for Windows
/// and Linux x86_64; other targets fail closed, so the tests that observe an
/// approval snapshot are gated to those targets and these helpers must follow
/// them. Ungated, they are dead code everywhere else, which this crate denies.
#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
fn approval_snapshot(io: &LocalFileIo, persistence: &Path) -> (Vec<PathBuf>, Option<Vec<u8>>) {
    let approved = io
        .approved_paths
        .lock()
        .expect("approval lock is available")
        .clone();
    let persisted = match fs::read(persistence) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("approval persistence can be read: {error}"),
    };
    (approved, persisted)
}

#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
fn assert_approval_snapshot_unchanged(
    io: &LocalFileIo,
    persistence: &Path,
    before: &(Vec<PathBuf>, Option<Vec<u8>>),
) {
    assert_eq!(approval_snapshot(io, persistence), before.clone());
}

#[test]
fn only_explicit_approval_grants_normalized_existing_or_missing_targets() {
    let temp = TestDir::new("exact-approval");
    let cache = temp.path().join("cache");
    let selected = temp.path().join("selected");
    fs::create_dir_all(&cache).expect("cache directory exists");
    fs::create_dir_all(&selected).expect("selected directory exists");
    let persistence = temp.path().join("approvals.json");
    let missing = selected.join("draft.ptz");
    let normalized_input = selected.join("pending").join("..").join("draft.ptz");

    let io = LocalFileIo::new(&cache, &persistence).expect("empty approval store opens");
    assert_code(
        io.authorize_save_target(&missing)
            .expect_err("authorization cannot self-approve"),
        "E_VALIDATION",
    );
    io.approve_path(&normalized_input)
        .expect("separate explicit approval persists");
    assert!(
        !missing.exists(),
        "approving a missing target creates no file"
    );

    let reopened = LocalFileIo::new(&cache, &persistence).expect("approval reloads");
    assert!(reopened.authorize_save_target(&missing).is_ok());
    assert_code(
        reopened
            .authorize_save_target(&selected.join("draft.ptz-sibling.ptz"))
            .expect_err("string-prefix siblings are not exact approvals"),
        "E_VALIDATION",
    );

    let existing = selected.join("existing.ptz");
    fs::write(&existing, b"selected existing file").expect("existing target is created");
    reopened
        .approve_path(&existing)
        .expect("existing target approval persists");
    assert!(reopened.authorize_save_target(&existing).is_ok());
}

#[test]
fn cache_descendants_are_trusted_but_siblings_and_non_ptz_paths_are_not() {
    let temp = TestDir::new("cache-boundary");
    let cache = temp.path().join("app-cache");
    let child = cache.join("projects");
    let sibling = temp.path().join("app-cache-sibling");
    fs::create_dir_all(&child).expect("cache child exists");
    fs::create_dir_all(&sibling).expect("prefix sibling exists");
    let io =
        LocalFileIo::new(&cache, temp.path().join("approvals.json")).expect("approval store opens");

    assert!(io.authorize_save_target(&child.join("draft.PTZ")).is_ok());
    assert_code(
        io.authorize_save_target(&sibling.join("draft.ptz"))
            .expect_err("string-prefix siblings are outside the cache root"),
        "E_VALIDATION",
    );
    assert_code(
        io.authorize_save_target(&child.join("draft.zip"))
            .expect_err("save extensions remain restricted to PTZ"),
        "E_VALIDATION",
    );
}

#[test]
fn approval_rejects_symlinked_ancestors_when_supported() {
    let temp = TestDir::new("symlink-ancestor");
    let cache = temp.path().join("cache");
    let real = temp.path().join("real");
    let link = temp.path().join("linked");
    fs::create_dir_all(&cache).expect("cache exists");
    fs::create_dir_all(&real).expect("real directory exists");
    #[cfg(windows)]
    let result = std::os::windows::fs::symlink_dir(&real, &link);
    #[cfg(unix)]
    let result = std::os::unix::fs::symlink(&real, &link);
    #[cfg(not(any(windows, unix)))]
    let result: std::io::Result<()> = Err(std::io::Error::other("symlink unsupported"));
    if result.is_err() {
        eprintln!("SKIP symlink rejection: host could not create a directory symlink: {result:?}");
        return;
    }
    eprintln!("Created directory symlink; exercising ancestor rejection.");

    let io =
        LocalFileIo::new(&cache, temp.path().join("approvals.json")).expect("approval store opens");
    assert_code(
        super::platform_read::open_read_file(&link.join("through-link.ptz"))
            .expect_err("handle-relative read rejects a symlinked ancestor"),
        "E_VALIDATION",
    );
    assert_code(
        io.approve_path(&link.join("through-link.ptz"))
            .expect_err("symlink ancestors are refused before trust is granted"),
        "E_VALIDATION",
    );
}

#[test]
fn malformed_persistence_and_write_failures_are_typed_and_never_grant_trust() {
    let temp = TestDir::new("persistence-errors");
    let cache = temp.path().join("cache");
    fs::create_dir_all(&cache).expect("cache exists");
    let malformed = temp.path().join("malformed.json");
    fs::write(&malformed, b"{").expect("malformed persistence is written");
    assert_code(
        LocalFileIo::new(&cache, &malformed).expect_err("malformed persistence is not ignored"),
        "E_VALIDATION",
    );

    let blocked_parent = temp.path().join("blocked-parent");
    let io = LocalFileIo::new(&cache, blocked_parent.join("approvals.json"))
        .expect("missing persistence parent is allowed initially");
    fs::write(&blocked_parent, b"regular file blocks mkdir")
        .expect("persistence parent is blocked after initialization");
    let selected = temp.path().join("selected.ptz");
    assert_code(
        io.approve_path(&selected)
            .expect_err("approval persistence failure is returned"),
        "E_IO",
    );
    assert_code(
        io.authorize_save_target(&selected)
            .expect_err("failed persistence does not grant trust"),
        "E_VALIDATION",
    );
}

#[cfg(windows)]
#[test]
fn approval_persistence_replaces_existing_file_on_windows() {
    let temp = TestDir::new("windows-persistence-replacement");
    let cache = temp.path().join("cache");
    let selected = temp.path().join("selected");
    fs::create_dir_all(&cache).expect("cache exists");
    fs::create_dir_all(&selected).expect("selected directory exists");
    let persistence = temp.path().join("approvals.json");
    let first = selected.join("first.ptz");
    let second = selected.join("second.ptz");
    let io = LocalFileIo::new(&cache, &persistence).expect("empty approval store opens");

    io.approve_path(&first)
        .expect("first approval is persisted");
    let initial_contents = fs::read(&persistence).expect("first persistence file exists");
    io.approve_path(&second)
        .expect("second approval atomically replaces the existing persistence file");
    let replaced_contents = fs::read(&persistence).expect("replacement persistence file exists");
    assert_ne!(replaced_contents, initial_contents);

    let reloaded = LocalFileIo::new(&cache, &persistence).expect("replacement reloads");
    assert!(reloaded.authorize_save_target(&first).is_ok());
    assert!(reloaded.authorize_save_target(&second).is_ok());
}

#[test]
#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
fn read_approved_returns_bytes_for_an_explicitly_approved_file() {
    let temp = TestDir::new("read-approved-success");
    let cache = temp.path().join("cache");
    fs::create_dir_all(&cache).expect("cache exists");
    let persistence = temp.path().join("approvals.json");
    let source = temp.path().join("selected.bin");
    fs::write(&source, b"approved bytes").expect("source file exists");
    let io = LocalFileIo::new(&cache, &persistence).expect("approval store opens");
    io.approve_path(&source).expect("source is approved");

    assert_eq!(
        io.read_approved(&source, 14)
            .expect("approved file is readable"),
        b"approved bytes"
    );
}

#[test]
#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
fn read_approved_rejects_unapproved_path_without_mutating_approval() {
    let temp = TestDir::new("read-approved-unapproved");
    let cache = temp.path().join("cache");
    fs::create_dir_all(&cache).expect("cache exists");
    let persistence = temp.path().join("approvals.json");
    let source = temp.path().join("cancelled-selection.bin");
    fs::write(&source, b"not approved").expect("source file exists");
    let io = LocalFileIo::new(&cache, &persistence).expect("approval store opens");
    // No approve_path call models a cancelled file-selection dialog.
    let before = approval_snapshot(&io, &persistence);

    assert_code(
        io.read_approved(&source, 32)
            .expect_err("unapproved file cannot be read"),
        "E_VALIDATION",
    );
    assert_approval_snapshot_unchanged(&io, &persistence, &before);
}

#[test]
#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
fn read_approved_rejects_a_symlink_at_an_approved_path_without_mutating_approval() {
    let temp = TestDir::new("read-approved-symlink");
    let cache = temp.path().join("cache");
    fs::create_dir_all(&cache).expect("cache exists");
    let persistence = temp.path().join("approvals.json");
    let approved_path = temp.path().join("selected.bin");
    let target = temp.path().join("target.bin");
    fs::write(&approved_path, b"original file").expect("approved file exists");
    fs::write(&target, b"symlink target").expect("symlink target exists");
    let io = LocalFileIo::new(&cache, &persistence).expect("approval store opens");
    io.approve_path(&approved_path)
        .expect("file path is approved before replacement");
    fs::remove_file(&approved_path).expect("original file is removed");
    #[cfg(windows)]
    let result = std::os::windows::fs::symlink_file(&target, &approved_path);
    #[cfg(unix)]
    let result = std::os::unix::fs::symlink(&target, &approved_path);
    #[cfg(not(any(windows, unix)))]
    let result: std::io::Result<()> = Err(std::io::Error::other("symlink unsupported"));
    if result.is_err() {
        eprintln!("SKIP symlink rejection: host could not create a file symlink: {result:?}");
        return;
    }
    let before = approval_snapshot(&io, &persistence);

    assert_code(
        super::platform_read::open_read_file(&approved_path)
            .expect_err("handle-relative read rejects a final symlink"),
        "E_VALIDATION",
    );
    assert_code(
        io.read_approved(&approved_path, 32)
            .expect_err("approved path cannot be redirected through a symlink"),
        "E_VALIDATION",
    );
    assert_approval_snapshot_unchanged(&io, &persistence, &before);
}

#[test]
#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
fn read_approved_rejects_non_regular_paths_without_mutating_approval() {
    let temp = TestDir::new("read-approved-non-regular");
    let cache = temp.path().join("cache");
    let directory = temp.path().join("selected-directory");
    fs::create_dir_all(&cache).expect("cache exists");
    fs::create_dir_all(&directory).expect("selected directory exists");
    let persistence = temp.path().join("approvals.json");
    let io = LocalFileIo::new(&cache, &persistence).expect("approval store opens");
    io.approve_path(&directory)
        .expect("directory path can be explicitly approved");
    let before = approval_snapshot(&io, &persistence);

    assert_code(
        io.read_approved(&directory, 32)
            .expect_err("directories are not regular files"),
        "E_VALIDATION",
    );
    assert_approval_snapshot_unchanged(&io, &persistence, &before);
}

#[test]
#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
fn read_approved_reports_missing_file_without_mutating_approval() {
    let temp = TestDir::new("read-approved-missing");
    let cache = temp.path().join("cache");
    fs::create_dir_all(&cache).expect("cache exists");
    let persistence = temp.path().join("approvals.json");
    let missing = temp.path().join("missing.bin");
    let io = LocalFileIo::new(&cache, &persistence).expect("approval store opens");
    io.approve_path(&missing)
        .expect("missing path can have a persisted exact-path approval");
    let before = approval_snapshot(&io, &persistence);

    assert_code(
        io.read_approved(&missing, 32)
            .expect_err("missing approved file cannot be read"),
        "E_IO",
    );
    assert_approval_snapshot_unchanged(&io, &persistence, &before);
}

#[test]
#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
fn read_approved_enforces_the_max_bytes_boundary_without_mutating_approval() {
    let temp = TestDir::new("read-approved-limit");
    let cache = temp.path().join("cache");
    fs::create_dir_all(&cache).expect("cache exists");
    let persistence = temp.path().join("approvals.json");
    let source = temp.path().join("boundary.bin");
    fs::write(&source, b"1234").expect("source file exists");
    let io = LocalFileIo::new(&cache, &persistence).expect("approval store opens");
    io.approve_path(&source).expect("source is approved");

    assert_eq!(
        io.read_approved(&source, 4)
            .expect("file size equal to limit is accepted"),
        b"1234"
    );
    let before = approval_snapshot(&io, &persistence);
    assert_code(
        io.read_approved(&source, 3)
            .expect_err("file larger than limit is rejected"),
        "E_RESOURCE_LIMIT",
    );
    assert_approval_snapshot_unchanged(&io, &persistence, &before);
}

#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
#[test]
fn opened_read_handle_survives_final_component_replacement() {
    let temp = TestDir::new("read-handle-final-replacement");
    let cache = temp.path().join("cache");
    fs::create_dir_all(&cache).expect("cache exists");
    let persistence = temp.path().join("approvals.json");
    let source = temp.path().join("selected.bin");
    let displaced = temp.path().join("displaced.bin");
    fs::write(&source, b"validated handle bytes").expect("source file exists");
    let io = LocalFileIo::new(&cache, &persistence).expect("approval store opens");
    io.approve_path(&source).expect("source is approved");
    let handle = super::platform_read::open_read_file(&source)
        .expect("approved path can be opened through platform handles");

    // Deterministic handle check after open; this does not force an adversarial
    // replacement in the concurrent interval before the final open completes.
    fs::rename(&source, &displaced).expect("open handle permits final path replacement");
    fs::write(&source, b"replacement bytes").expect("replacement file is installed");

    assert_eq!(
        super::read_bounded_file(handle, 64).expect("opened handle remains readable"),
        b"validated handle bytes"
    );
}

#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
#[test]
fn opened_parent_handle_survives_ancestor_replacement() {
    let temp = TestDir::new("read-handle-ancestor-replacement");
    let cache = temp.path().join("cache");
    let parent = temp.path().join("selected");
    let displaced = temp.path().join("selected-original");
    fs::create_dir_all(&cache).expect("cache exists");
    fs::create_dir_all(&parent).expect("selected directory exists");
    let persistence = temp.path().join("approvals.json");
    let source = parent.join("selected.bin");
    fs::write(&source, b"original ancestor bytes").expect("source file exists");
    let io = LocalFileIo::new(&cache, &persistence).expect("approval store opens");
    io.approve_path(&source).expect("source is approved");
    let parent_handle = super::platform_read::open_directory_path(&parent)
        .expect("parent is opened as a directory handle");

    // Replacement after this handle opens verifies child resolution is relative
    // to the original directory, not the new pathname target. It does not inject
    // a concurrent swap during the earlier root-to-parent walk.
    fs::rename(&parent, &displaced).expect("open directory handle permits ancestor replacement");
    fs::create_dir_all(&parent).expect("replacement ancestor is installed");
    fs::write(parent.join("selected.bin"), b"replacement ancestor bytes")
        .expect("replacement file exists");
    let handle = super::platform_read::open_file_at(&parent_handle, OsStr::new("selected.bin"))
        .expect("child is opened relative to the original parent handle");

    assert_eq!(
        super::read_bounded_file(handle, 64).expect("original parent handle remains usable"),
        b"original ancestor bytes"
    );
}
