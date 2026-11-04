//! Which archives the recovery prompt offers, and which it leaves in the store.

use super::*;

/// An archive that predates the file already on screen cannot hold work the user
/// has not got in front of them, so it is not offered, and suppression leaves the
/// archive in the store.
#[gpui_kit::test]
fn an_older_archive_of_the_open_document_is_not_offered(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let source = directory.path().join("named.ptz");
    std::fs::write(&source, super::commands::multi_layer_archive_with(false))
        .expect("the named archive is written");
    let file_io = LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
        .expect("the file boundary opens");
    file_io
        .approve_path(&source)
        .expect("the named archive is approved");
    let io: Arc<dyn FileIo> = Arc::new(file_io);

    let cache = directory.path().join("older-doc-cache");
    let autosave = cache.join("autosave");
    std::fs::create_dir_all(&autosave).expect("the autosave directory");
    std::fs::write(
        autosave.join("multi-doc.ptz"),
        super::commands::multi_layer_archive_with(false),
    )
    .expect("the older autosave archive is written");
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0);
    std::fs::write(
        autosave.join("manifest.json"),
        format!(r#"{{"multi-doc":"Older edits|{}"}}"#, now_ms - 60_000),
    )
    .expect("the manifest is written");

    let (window, content) = mount_app_with_cache_root(cx, source, io, cache.clone());
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let app = content.read(cx);
        let state = app.state_for_test().read(cx);
        assert_eq!(
            app.active_document_id_for_test().as_deref(),
            Some("multi-doc"),
            "the document named on the command line is the one on screen"
        );
        assert_eq!(
            state.active_dialog, None,
            "an archive older than the file on screen opens no prompt"
        );
        assert!(
            state.recovery_entries.is_empty(),
            "nothing is offered for a document that is already on screen in a newer state"
        );
    })
    .expect("test window remains open");

    assert!(
        cache.join("autosave").join("multi-doc.ptz").exists(),
        "suppressing the offer leaves the archive on disk, where only the 7-day cleanup bounds it"
    );
}

/// Discard answers the prompt, not the whole store. The suppressed archive from
/// `an_older_archive_of_the_open_document_is_not_offered` is still on disk when
/// the prompt opens for a different document, and the user never saw that row,
/// so declining what was listed must not delete what was not.
#[gpui_kit::test]
fn discarding_the_prompt_keeps_the_archive_it_never_listed(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let source = directory.path().join("named.ptz");
    std::fs::write(&source, super::commands::multi_layer_archive_with(false))
        .expect("the named archive is written");
    let file_io = LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
        .expect("the file boundary opens");
    file_io
        .approve_path(&source)
        .expect("the named archive is approved");
    let io: Arc<dyn FileIo> = Arc::new(file_io);

    let cache = directory.path().join("mixed-doc-cache");
    let autosave = cache.join("autosave");
    std::fs::create_dir_all(&autosave).expect("the autosave directory");
    for doc in ["multi-doc", "other-doc"] {
        std::fs::write(
            autosave.join(format!("{doc}.ptz")),
            super::commands::multi_layer_archive_with(false),
        )
        .expect("an autosave archive is written");
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0);
    std::fs::write(
        autosave.join("manifest.json"),
        format!(
            r#"{{"multi-doc":"Older edits|{}","other-doc":"Other edits|{}"}}"#,
            now_ms - 60_000,
            now_ms
        ),
    )
    .expect("the manifest lists both archives");

    let (window, content) = mount_app_with_cache_root(cx, source, io, cache.clone());
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let listed = content.update(cx, |app, cx| {
            app.state_for_test().read(cx).recovery_entries.len()
        });
        assert_eq!(
            listed, 1,
            "the prompt lists the archive with no session on screen and nothing else"
        );
        window.click("recovery-discard", cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");

    assert!(
        !cache.join("autosave").join("other-doc.ptz").exists(),
        "the archive the prompt showed is the one Discard removed"
    );
    assert!(
        cache.join("autosave").join("multi-doc.ptz").exists(),
        "Discard deleted a suppressed archive the user was never shown"
    );
}

/// The contract's other half: when recency cannot be established at all, the
/// archive is not the thing that is uncertain, so the offer stands and the
/// document on screen keeps its tab.
#[gpui_kit::test]
fn an_archive_without_a_timestamp_is_still_offered_for_an_open_document(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let source = directory.path().join("named.ptz");
    std::fs::write(&source, super::commands::multi_layer_archive_with(false))
        .expect("the named archive is written");
    let file_io = LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
        .expect("the file boundary opens");
    file_io
        .approve_path(&source)
        .expect("the named archive is approved");
    let io: Arc<dyn FileIo> = Arc::new(file_io);

    let cache = directory.path().join("unstamp-cache");
    let autosave = cache.join("autosave");
    std::fs::create_dir_all(&autosave).expect("the autosave directory");
    std::fs::write(
        autosave.join("multi-doc.ptz"),
        super::commands::multi_layer_archive_with(true),
    )
    .expect("the unstamped autosave archive is written");
    std::fs::write(
        autosave.join("manifest.json"),
        r#"{"multi-doc":"Undated edits"}"#,
    )
    .expect("a manifest with no timestamp is written");

    let (window, content) = mount_app_with_cache_root(cx, source, io, cache.clone());
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let app = content.read(cx);
        let state = app.state_for_test().read(cx);
        assert_eq!(
            state.active_dialog,
            Some(crate::state::DialogId::Recovery),
            "an archive whose age cannot be read is not silently dropped"
        );
        assert_eq!(
            state.recovery_entries.len(),
            1,
            "the undated archive stays offered until the user decides"
        );
        assert_eq!(app.open_tab_ids_for_test(), vec!["multi-doc"]);
    })
    .expect("test window remains open");

    assert!(
        cache.join("autosave").join("multi-doc.ptz").exists(),
        "keeping the offer leaves the archive in place"
    );
}

/// The suppression rule reads "same or newer", so the boundary case is the
/// archive stamped at the file's own write time: it cannot hold work the user
/// does not already have in front of them.
#[gpui_kit::test]
fn an_archive_stamped_at_the_open_documents_write_time_is_not_offered(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let source = directory.path().join("named.ptz");
    std::fs::write(&source, super::commands::multi_layer_archive_with(false))
        .expect("the named archive is written");
    let file_io = LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
        .expect("the file boundary opens");
    file_io
        .approve_path(&source)
        .expect("the named archive is approved");
    // Stamp the archive with the number the filter will read back, so the two
    // times are equal rather than merely close.
    let saved_at = file_io
        .modified_ms(&source)
        .expect("the boundary reports the file's write time");
    let io: Arc<dyn FileIo> = Arc::new(file_io);

    let cache = directory.path().join("same-time-cache");
    let autosave = cache.join("autosave");
    std::fs::create_dir_all(&autosave).expect("the autosave directory");
    std::fs::write(
        autosave.join("multi-doc.ptz"),
        super::commands::multi_layer_archive_with(false),
    )
    .expect("the archive of the same state is written");
    std::fs::write(
        autosave.join("manifest.json"),
        format!(r#"{{"multi-doc":"Same state|{saved_at}"}}"#),
    )
    .expect("the manifest is stamped at the file's write time");

    let (window, content) = mount_app_with_cache_root(cx, source, io, cache.clone());
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let app = content.read(cx);
        let state = app.state_for_test().read(cx);
        assert_eq!(
            app.active_document_id_for_test().as_deref(),
            Some("multi-doc"),
            "the document named on the command line is the one on screen"
        );
        assert!(
            state.recovery_entries.is_empty(),
            "an archive stamped at the file's own write time is the same state, so nothing is \
             offered: {:?}",
            state.recovery_entries
        );
        assert_eq!(
            state.active_dialog, None,
            "the same-state archive opens no prompt"
        );
    })
    .expect("test window remains open");

    assert!(
        cache.join("autosave").join("multi-doc.ptz").exists(),
        "suppressing the offer leaves the archive on disk"
    );
}

/// An archive newer than the file on screen holds work nothing else carries. It
/// is offered, marked against the document it acts on, and neither copy is
/// removed before the user answers.
#[gpui_kit::test]
fn a_newer_archive_of_the_open_document_keeps_both_copies(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let source = directory.path().join("named.ptz");
    std::fs::write(&source, super::commands::multi_layer_archive_with(false))
        .expect("the named archive is written");
    let file_io = LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
        .expect("the file boundary opens");
    file_io
        .approve_path(&source)
        .expect("the named archive is approved");
    let io: Arc<dyn FileIo> = Arc::new(file_io);

    let cache = directory.path().join("newer-doc-cache");
    let autosave = cache.join("autosave");
    std::fs::create_dir_all(&autosave).expect("the autosave directory");
    let archive = autosave.join("multi-doc.ptz");
    // The top layer carries a lock the file on screen does not, so this is a
    // different project rather than a second copy of the same one.
    std::fs::write(&archive, super::commands::multi_layer_archive_with(true))
        .expect("the newer autosave archive is written");
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0);
    std::fs::write(
        autosave.join("manifest.json"),
        format!(r#"{{"multi-doc":"Newer edits|{}"}}"#, now_ms + 60_000),
    )
    .expect("the manifest is stamped after the file on screen");

    let (window, content) = mount_app_with_cache_root(cx, source, io, cache.clone());
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let app = content.read(cx);
        let state = app.state_for_test().read(cx);
        assert_eq!(
            app.active_document_id_for_test().as_deref(),
            Some("multi-doc"),
            "the document named on the command line is still the one on screen"
        );
        assert_eq!(
            state.tabs.len(),
            1,
            "the offer alone never adds a tab for the document it names"
        );
        assert_eq!(
            state.active_dialog,
            Some(crate::state::DialogId::Recovery),
            "an archive newer than the file on screen is never suppressed"
        );
        let entries = state.recovery_entries.clone();
        assert_eq!(
            entries
                .iter()
                .map(|entry| (entry.doc_id.as_str(), entry.already_open))
                .collect::<Vec<_>>(),
            vec![("multi-doc", true)],
            "the offer names the document already on screen, so the choice reads against it: \
             {entries:?}"
        );
        assert_eq!(
            state.status_error, None,
            "offering a newer archive reports no failure"
        );
    })
    .expect("test window remains open");

    assert!(
        archive.is_file(),
        "the newer archive survives the offer untouched"
    );
    assert!(
        directory.path().join("named.ptz").is_file(),
        "the file the session opened from survives the offer untouched"
    );
    assert_ne!(
        std::fs::read(&archive).expect("the archive reads"),
        std::fs::read(directory.path().join("named.ptz")).expect("the source reads"),
        "the archive holds a different project than the one on screen"
    );
    let listed = crate::services::autosave::AutosaveStore::new(&cache)
        .list()
        .expect("the manifest reads");
    assert_eq!(
        listed
            .iter()
            .map(|entry| entry.doc_id())
            .collect::<Vec<_>>(),
        vec!["multi-doc"],
        "the offer leaves the entry in the manifest"
    );
}

/// A boundary that reads files but answers no timestamp for them: the shape a
/// filesystem or platform that withholds a file's write time produces.
struct NoTimestampFileIo(LocalFileIo);

impl FileIo for NoTimestampFileIo {
    fn approve_path(&self, raw_path: &std::path::Path) -> Result<(), ServiceError> {
        self.0.approve_path(raw_path)
    }

    fn read_approved(
        &self,
        raw_path: &std::path::Path,
        max_bytes: u64,
    ) -> Result<Vec<u8>, ServiceError> {
        self.0.read_approved(raw_path, max_bytes)
    }

    fn modified_ms(&self, _raw_path: &std::path::Path) -> Result<u64, ServiceError> {
        Err(ServiceError::Io(std::io::Error::other(
            "this platform reports no file time",
        )))
    }

    fn authorize_save_target(
        &self,
        raw_path: &std::path::Path,
    ) -> Result<SaveTarget, ServiceError> {
        self.0.authorize_save_target(raw_path)
    }
}

/// Recency that cannot be established is not evidence of staleness: the entry
/// stays offered, so the archive of unsaved work keeps its explicit option.
#[gpui_kit::test]
fn an_unreadable_file_time_keeps_the_archive_of_the_open_document_offered(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let source = directory.path().join("named.ptz");
    std::fs::write(&source, super::commands::multi_layer_archive_with(false))
        .expect("the named archive is written");
    let boundary = LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
        .expect("the file boundary opens");
    boundary
        .approve_path(&source)
        .expect("the named archive is approved");
    let io: Arc<dyn FileIo> = Arc::new(NoTimestampFileIo(boundary));

    let cache = directory.path().join("no-time-cache");
    let autosave = cache.join("autosave");
    std::fs::create_dir_all(&autosave).expect("the autosave directory");
    std::fs::write(
        autosave.join("multi-doc.ptz"),
        super::commands::multi_layer_archive_with(true),
    )
    .expect("the autosave archive is written");
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0);
    // Older than the file on screen: with a readable file time this entry would
    // be suppressed, so only the missing signal can be what keeps it offered.
    std::fs::write(
        autosave.join("manifest.json"),
        format!(r#"{{"multi-doc":"Undated on disk|{}"}}"#, now_ms - 60_000),
    )
    .expect("the manifest is written");

    let (window, content) = mount_app_with_cache_root(cx, source, io, cache.clone());
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let app = content.read(cx);
        let state = app.state_for_test().read(cx);
        assert_eq!(
            state.active_dialog,
            Some(crate::state::DialogId::Recovery),
            "a file time the platform will not give is not evidence that the file is newer"
        );
        let entries = state.recovery_entries.clone();
        assert_eq!(
            entries
                .iter()
                .map(|entry| (entry.doc_id.as_str(), entry.already_open))
                .collect::<Vec<_>>(),
            vec![("multi-doc", true)],
            "the entry stays offered and is marked against the document on screen: {entries:?}"
        );
        assert_eq!(app.open_tab_ids_for_test(), vec!["multi-doc"]);
    })
    .expect("test window remains open");

    assert!(
        cache.join("autosave").join("multi-doc.ptz").is_file(),
        "keeping the offer preserves the archive"
    );
}
