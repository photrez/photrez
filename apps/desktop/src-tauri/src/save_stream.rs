// SPDX-License-Identifier: AGPL-3.0-or-later
// --- Streaming Project Save ---
//
// save_project_streaming_* commands: incremental Zip-write sessions with
// TTL-based pruning (orphan cleanup when the frontend dies mid-save).
//
// The document payload is SERIALIZED HERE, not in the host: `document` arrives
// as a JSON value, is parsed into the typed `photrez_core::ptz_document::PtzDocument`
// (so a document that does not match the on-disk contract fails the save
// instead of being written), then re-serialized by that type and written into
// the ZIP. See `write_document_json_into_zip` for the atomicity contract.

use photrez_core::ptz_document::PtzDocument;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::file_io::{check_path_trusted, TrustedPathsState};
use crate::response::{
    err_response, error_value, ok_response, validate_path_extension, validate_path_safe,
};

// ── Streaming Save State ──

/// A single active Zip-write session — created by `begin`, consumed by `end`/`cancel`.
pub(crate) struct StreamingSaveSession {
    tmp_path: PathBuf,
    final_path: PathBuf,
    zip: Option<zip::ZipWriter<std::fs::File>>,
    created_at: Instant,
}

/// Global map of handle → active streaming save sessions.
pub(crate) struct StreamingSaveState {
    pub(crate) sessions: Mutex<HashMap<String, StreamingSaveSession>>,
}

impl Default for StreamingSaveState {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

/// Streaming save sessions that are not touched for this long are pruned
/// (zip dropped, temp file deleted) — guards against orphaned sessions and
/// `.tmp` accumulation when the frontend dies mid-save.
const STREAMING_SESSION_TTL: Duration = Duration::from_secs(600); // 10 minutes

/// Drop and delete every session older than `STREAMING_SESSION_TTL`.
fn prune_expired_sessions(sessions: &mut HashMap<String, StreamingSaveSession>) {
    let now = Instant::now();
    sessions.retain(|_, s| {
        if now.duration_since(s.created_at) >= STREAMING_SESSION_TTL {
            drop(s.zip.take());
            let _ = std::fs::remove_file(&s.tmp_path);
            false
        } else {
            true
        }
    });
}

// ── Document payload: serialize in Rust, then write into the ZIP ──

/// Write already-serialized `document.json` bytes into the in-progress ZIP.
///
/// Split out from the command so it is headless-testable: it needs no
/// `tauri::State`, only a zip writer.
///
/// ORDERING CONTRACT: the caller MUST have serialized the document already
/// (`prepare_save` runs before any file exists) and MUST delete the temp file
/// if this returns `Err`. The function itself does no serialization, so it
/// cannot fail on document content -- only on ZIP/filesystem errors.
fn write_document_json_into_zip<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    document_json: &str,
) -> Result<(), Value> {
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("document.json", options)
        .map_err(|e| error_value("E_IO", &format!("Failed to start document.json: {}", e)))?;
    zip.write_all(document_json.as_bytes())
        .map_err(|e| error_value("E_IO", &format!("Failed to write document.json: {}", e)))
}

/// Parse the host's document value into the typed payload and serialize the
/// exact bytes that go on disk. This is the single owner of the `.ptz`
/// `document.json` content in production.
///
/// A parse or serialize failure returns `Err`, and the caller aborts before
/// the ZIP is finalized, so the target path is never replaced by a document
/// that Rust cannot read back.
fn serialize_document_json(document: &Value) -> Result<String, Value> {
    let doc: PtzDocument = serde_json::from_value(document.clone()).map_err(|e| {
        error_value(
            "E_VALIDATION",
            &format!("Document does not match the .ptz format: {}", e),
        )
    })?;
    doc.to_json().map_err(|e| {
        error_value(
            "E_INTERNAL",
            &format!("Failed to serialize document: {}", e),
        )
    })
}

/// Begin a streaming save: create the temp file, write `document.json`, and
/// return a handle for the per-layer writes.
///
/// `document` is the canonical document as a JSON value (NOT a pre-serialized
/// string): Rust owns the serialization of `document.json`.
///
/// ASYMMETRY: Rust owns the WRITE, TypeScript owns the READ -- the loader in
/// `editorOpenImage.ts` is `JSON.parse` plus `restoreSnapshot`, and there is no
/// Rust read path for `document.json`. So nothing at runtime enforces that the
/// writer and the reader agree on the shape; only the round-trip tests do
/// (`ptz_document.rs` and `ptzWriterGolden.test.ts`).
///
/// Everything that must succeed BEFORE the filesystem is touched: path
/// validation and document serialization.
///
/// Split from the command so the ordering guarantee is testable. A save that
/// cannot produce a valid document must not create a temp file, and that
/// guarantee is only provable against a function that owns the whole
/// pre-filesystem step.
fn prepare_save(
    path: &str,
    document: &Value,
    trusted: &TrustedPathsState,
) -> Result<(PathBuf, String), Value> {
    validate_path_extension(path, &["ptz"], "save project")?;
    let safe_path = validate_path_safe(path, "save project")?;
    check_path_trusted(trusted, &safe_path)?;
    let document_json = serialize_document_json(document)?;
    Ok((safe_path, document_json))
}

#[tauri::command]
pub(crate) fn save_project_streaming_begin(
    path: String,
    document: Value,
    state: tauri::State<'_, StreamingSaveState>,
    trusted: tauri::State<'_, TrustedPathsState>,
) -> Result<Value, Value> {
    // Serialize + validate BEFORE touching the filesystem: a document that
    // does not match the format contract must not create a temp file at all.
    let (path, document_json) = prepare_save(&path, &document, &trusted)?;

    // Ensure parent directory exists.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| error_value("E_IO", &format!("Failed to create directory: {}", e)))?;
    }

    // Temp path (same dir as final, for atomic rename).
    let mut tmp_path = path.clone();
    let tmp_name = format!(
        "{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
    );
    tmp_path.set_file_name(&tmp_name);

    // Create temp file (overwrites any stale .tmp from a previous crash).
    let file = std::fs::File::create(&tmp_path)
        .map_err(|e| error_value("E_IO", &format!("Failed to create temp file: {}", e)))?;

    let mut zip = zip::ZipWriter::new(file);
    if let Err(e) = write_document_json_into_zip(&mut zip, &document_json) {
        // Do not leave a half-written temp file behind when the document
        // could not be written into it.
        drop(zip);
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }

    let handle_id = uuid::Uuid::new_v4().to_string();
    let mut sessions = state.sessions.lock().unwrap_or_else(|e| e.into_inner());
    prune_expired_sessions(&mut sessions);
    sessions.insert(
        handle_id.clone(),
        StreamingSaveSession {
            tmp_path,
            final_path: path,
            zip: Some(zip),
            created_at: Instant::now(),
        },
    );

    ok_response(serde_json::json!({ "handle_id": handle_id }))
}

/// Write one layer's PNG bytes to the in-progress ZIP file.
/// Uses raw IPC (Uint8Array body + headers) — zero base64 overhead.
#[tauri::command]
pub(crate) fn save_project_streaming_write_layer(
    request: tauri::ipc::Request<'_>,
    state: tauri::State<'_, StreamingSaveState>,
) -> Result<Value, Value> {
    // Read raw binary body (PNG bytes, zero encoding overhead).
    let tauri::ipc::InvokeBody::Raw(data) = request.body() else {
        return err_response("E_VALIDATION", "Expected raw binary body");
    };

    // Parse metadata from headers.
    let handle_id = request
        .headers()
        .get("handle-id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| error_value("E_VALIDATION", "Missing handle-id header"))?
        .to_string();

    let layer_id = request
        .headers()
        .get("layer-id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| error_value("E_VALIDATION", "Missing layer-id header"))?
        .to_string();

    let mut sessions = state.sessions.lock().unwrap_or_else(|e| e.into_inner());
    prune_expired_sessions(&mut sessions);
    let session = sessions.get_mut(&handle_id).ok_or_else(|| {
        error_value(
            "E_VALIDATION",
            "Invalid handle_id: session not found or expired",
        )
    })?;

    let zip = session
        .zip
        .as_mut()
        .ok_or_else(|| error_value("E_INTERNAL", "Session already ended or cancelled"))?;

    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let zip_path = format!("layers/{}.png", layer_id);
    zip.start_file(&zip_path, options).map_err(|e| {
        error_value(
            "E_IO",
            &format!("Failed to start layer {}: {}", layer_id, e),
        )
    })?;
    zip.write_all(data).map_err(|e| {
        error_value(
            "E_IO",
            &format!("Failed to write layer {}: {}", layer_id, e),
        )
    })?;

    ok_response(serde_json::json!({ "written": layer_id }))
}

/// Finish the ZIP, fsync it, and atomically rename it over the target.
///
/// Split out so the finalize-and-cleanup contract is testable without
/// `tauri::State`. On ANY failure the temp file is deleted here: the caller
/// keeps the session so a host-side cancel can still resolve, but the orphan
/// must not depend on that cancel arriving.
fn finalize_to_disk(
    zip: zip::ZipWriter<std::fs::File>,
    tmp_path: &std::path::Path,
    final_path: &std::path::Path,
) -> Result<(), Value> {
    let attempt = (|| -> std::io::Result<()> {
        // Finish ZipWriter (close central directory, finalize file).
        let file = zip.finish()?;
        // fsync data + directory for atomic durability.
        file.sync_all()?;
        if let Some(parent) = final_path.parent() {
            if let Ok(dir) = std::fs::File::open(parent) {
                let _ = dir.sync_all();
            }
        }
        // Atomic rename — either tmp replaces path completely, or the rename
        // fails and path is untouched. On Windows this is atomic if both paths
        // are on the same volume (they are, since tmp is in the same dir).
        std::fs::rename(tmp_path, final_path)
    })();

    if let Err(e) = attempt {
        // The temp file exists on every path that got as far as `File::create`,
        // and a finalize failure after that point would otherwise strand it in
        // the user's save directory with no handle left to clean it up.
        let _ = std::fs::remove_file(tmp_path);
        return Err(error_value(
            "E_IO",
            &format!("Failed to finalize save: {}", e),
        ));
    }
    Ok(())
}

/// The body of `save_project_streaming_end`, taking the session map directly so
/// the finalize contract is testable headlessly.
///
/// The session is retained until the rename succeeds. Removing it first would
/// strand the temp file on a finalize failure: the host's compensating cancel
/// would get "session not found", TTL pruning could not reach the entry, and a
/// `<name>.ptz.tmp` would be left in the user's save directory permanently.
fn end_session(
    sessions: &mut HashMap<String, StreamingSaveSession>,
    handle_id: &str,
) -> Result<PathBuf, Value> {
    prune_expired_sessions(sessions);

    // Take the zip, but leave the session in the map so a failure below is
    // still cleanable by a host cancel or by TTL pruning.
    let (tmp_path, final_path, zip) = {
        let session = sessions.get_mut(handle_id).ok_or_else(|| {
            error_value(
                "E_VALIDATION",
                "Invalid handle_id: session not found or expired",
            )
        })?;
        let zip = session.zip.take().ok_or_else(|| {
            error_value("E_INTERNAL", "Session zip already consumed — double end?")
        })?;
        (session.tmp_path.clone(), session.final_path.clone(), zip)
    };

    finalize_to_disk(zip, &tmp_path, &final_path)?;

    // Only now is the session genuinely finished.
    sessions.remove(handle_id);
    Ok(final_path)
}

/// Finalize the streaming save — close ZIP, fsync, atomic rename.
#[tauri::command]
pub(crate) fn save_project_streaming_end(
    handle_id: String,
    state: tauri::State<'_, StreamingSaveState>,
) -> Result<Value, Value> {
    let mut sessions = state.sessions.lock().unwrap_or_else(|e| e.into_inner());
    let path = end_session(&mut sessions, &handle_id)?;
    ok_response(serde_json::json!({ "path": path }))
}

/// Cancel an in-progress streaming save — drop zip, delete temp file.
#[tauri::command]
pub(crate) fn save_project_streaming_cancel(
    handle_id: String,
    state: tauri::State<'_, StreamingSaveState>,
) -> Result<Value, Value> {
    let mut sessions = state.sessions.lock().unwrap_or_else(|e| e.into_inner());
    prune_expired_sessions(&mut sessions);
    let session = sessions.remove(&handle_id).ok_or_else(|| {
        error_value(
            "E_VALIDATION",
            "Invalid handle_id: session not found or expired",
        )
    })?;

    // Drop the ZipWriter — closes without finalizing (corrupted zip, cleaned up).
    drop(session.zip);

    // Delete the temp file.
    let _ = std::fs::remove_file(&session.tmp_path);

    ok_response(serde_json::json!({ "cancelled": true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use photrez_core::ptz_fixtures::{
        LEGACY_TEXT_LAYER_MODEL, MIXED_LAYER_TYPES_MODEL, REAL_DUMPED_MODEL,
    };
    use std::io::Read;

    /// Run the exact serialization + ZIP-write steps the command performs,
    /// without the `tauri::State` the command signature requires.
    fn save_document_to_zip(document_json: &str) -> Vec<u8> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        write_document_json_into_zip(&mut zip, document_json).expect("document.json writes");
        let cursor = zip.finish().expect("zip finalizes");
        cursor.into_inner()
    }

    /// Read one entry out of a ZIP archive.
    fn read_zip_entry(archive: &[u8], name: &str) -> String {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive)).expect("valid zip");
        let mut entry = zip.by_name(name).expect("entry exists");
        let mut out = String::new();
        entry.read_to_string(&mut out).expect("entry decodes");
        out
    }

    #[test]
    fn production_save_path_round_trips_the_real_dumped_model() {
        // Serialize exactly as the command does, from the host's document value.
        let host_document: Value = serde_json::from_str(REAL_DUMPED_MODEL).unwrap();
        let document_json = serialize_document_json(&host_document).expect("serializes");

        let archive = save_document_to_zip(&document_json);
        let read_back = read_zip_entry(&archive, "document.json");

        // The ZIP carries the same bytes the writer produced.
        assert_eq!(read_back, document_json, "zip entry must match the writer");

        // And they parse back into the same document the loader will read.
        let reparsed = PtzDocument::from_json(&read_back).expect("read-back parses");
        let original = PtzDocument::from_json(REAL_DUMPED_MODEL).unwrap();

        assert_eq!(reparsed.id, original.id);
        assert_eq!(reparsed.name, original.name);
        assert_eq!(reparsed.width, original.width);
        assert_eq!(reparsed.height, original.height);
        assert_eq!(reparsed.active_layer_id, original.active_layer_id);
        assert_eq!(reparsed.viewport, original.viewport);
        assert_eq!(reparsed.selection, original.selection);
        assert_eq!(reparsed.dirty, original.dirty);
        assert_eq!(reparsed.layers, original.layers, "every layer survives");
    }

    #[test]
    fn production_save_path_preserves_each_layer_field() {
        let host_document: Value = serde_json::from_str(REAL_DUMPED_MODEL).unwrap();
        let read_back = read_zip_entry(
            &save_document_to_zip(&serialize_document_json(&host_document).unwrap()),
            "document.json",
        );
        let reparsed = PtzDocument::from_json(&read_back).unwrap();

        let painted = reparsed
            .layers
            .iter()
            .find(|l| l.name == "Painted")
            .unwrap();
        assert_eq!(painted.id, "layer-ryyklho2");
        assert_eq!(painted.opacity, 0.75);
        assert!(painted.visible);
        assert!(painted.locked);
        assert_eq!(painted.lock_transparency, Some(true));
        assert_eq!(
            painted.blend_mode,
            photrez_core::canonical_model::BlendMode::Multiply
        );
        assert_eq!(
            painted.layer_type,
            photrez_core::canonical_model::LayerType::Raster
        );

        let background = reparsed
            .layers
            .iter()
            .find(|l| l.name == "Background")
            .unwrap();
        assert_eq!(background.is_background, Some(true));
        assert_eq!(background.lock_position, Some(true));
        assert_eq!(background.lock_rotation, Some(true));
    }

    #[test]
    fn production_save_path_writes_the_v3_header_and_null_bitmaps() {
        let host_document: Value = serde_json::from_str(REAL_DUMPED_MODEL).unwrap();
        let read_back = read_zip_entry(
            &save_document_to_zip(&serialize_document_json(&host_document).unwrap()),
            "document.json",
        );
        let v: Value = serde_json::from_str(&read_back).unwrap();

        assert_eq!(v["format"], Value::String("photrez-ptz".to_string()));
        assert_eq!(v["version"], Value::Number(3.into()));
        for layer in v["layers"].as_array().unwrap() {
            assert_eq!(layer["imageBitmap"], Value::Null);
            assert_eq!(layer["baseImageBitmap"], Value::Null);
        }
    }

    /// Fail loudly: a document outside the format contract must be rejected,
    /// so no archive is produced and the target path is left untouched.
    #[test]
    fn production_save_path_rejects_a_document_it_could_not_read_back() {
        let bad = serde_json::json!({
            "id": "d", "name": "n", "width": 10, "height": 10,
            "layers": [{
                "id": "l", "name": "L", "type": "raster", "visible": true,
                "opacity": "not-a-number", "locked": false, "blendMode": "normal",
                "transform": {"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0,"flipH":false,"flipV":false},
                "width": 10, "height": 10
            }]
        });
        let err = serialize_document_json(&bad).expect_err("must reject");
        assert_eq!(
            err["error"]["code"],
            Value::String("E_VALIDATION".to_string())
        );
    }

    #[test]
    fn production_save_path_saves_a_text_layer_with_missing_late_fields() {
        // The regression this guards: a file whose text layer predates most
        // TextData fields loads and renders fine (the loader never normalises
        // textData), and would then fail its next save with E_VALIDATION,
        // writing nothing — silently, because autosave folds the failure into
        // a status flag.
        let host_document: Value = serde_json::from_str(LEGACY_TEXT_LAYER_MODEL).unwrap();
        let document_json =
            serialize_document_json(&host_document).expect("a legacy text layer must still save");
        let reread = PtzDocument::from_json(&document_json).expect("re-readable");

        let td = reread.layers[0].text_data.as_ref().expect("textData");
        assert_eq!(td.content, "legacy", "carried fields are untouched");
        assert_eq!(td.font_size, 36.0);
        assert_eq!(td.letter_spacing, 0.0, "absent field defaults");
        assert_eq!(td.uppercase, Some(false), "absent optional defaults");
    }

    #[test]
    fn production_save_path_saves_all_four_layer_kinds() {
        let host_document: Value = serde_json::from_str(MIXED_LAYER_TYPES_MODEL).unwrap();
        let read_back = read_zip_entry(
            &save_document_to_zip(&serialize_document_json(&host_document).unwrap()),
            "document.json",
        );
        let reparsed = PtzDocument::from_json(&read_back).expect("re-readable");

        assert_eq!(reparsed.layers.len(), 4);
        let shape = reparsed.layers.iter().find(|l| l.name == "Star").unwrap();
        assert_eq!(shape.shape_params.as_ref().unwrap().fill.color, "#E15A17");
        let text = reparsed.layers.iter().find(|l| l.name == "Title").unwrap();
        assert_eq!(text.text_data.as_ref().unwrap().content, "Hello");
        assert_eq!(
            text.text_data.as_ref().unwrap().stroke.align,
            Some(photrez_core::canonical_model::TextStrokeAlign::Inside)
        );
        let adjusted = reparsed
            .layers
            .iter()
            .find(|l| l.name == "Adjusted")
            .unwrap();
        assert_eq!(adjusted.basic_adjustment.as_ref().unwrap().contrast, -33.25);
    }

    #[test]
    fn production_save_path_rejects_malformed_json() {
        let err = serialize_document_json(&Value::String("not a document".to_string()))
            .expect_err("must reject");
        assert_eq!(
            err["error"]["code"],
            Value::String("E_VALIDATION".to_string())
        );
    }

    /// A trusted-path state for a scratch dir under the OS temp dir.
    fn trusted_for(dir: &std::path::Path, target: &std::path::Path) -> TrustedPathsState {
        let _ = std::fs::create_dir_all(dir);
        let trusted = TrustedPathsState::new(
            std::env::temp_dir().join("photrez_trusted_stream_test"),
            dir.join("trusted.json"),
        );
        trusted.trust_path(target.to_str().unwrap());
        trusted
    }

    /// The ordering guarantee: a document that cannot be serialized is
    /// rejected by the pre-filesystem step, so no temp file is created and
    /// the target path is never touched.
    #[test]
    fn a_bad_document_is_rejected_before_any_file_is_created() {
        let dir = std::env::temp_dir().join("photrez_prepare_save_test");
        let target = dir.join("out.ptz");
        let _ = std::fs::remove_file(&target);
        let tmp = dir.join("out.ptz.tmp");
        let _ = std::fs::remove_file(&tmp);

        let trusted = trusted_for(&dir, &target);
        let bad = serde_json::json!({
            "id": "d", "name": "n", "width": 10, "height": 10,
            "layers": [{
                "id": "l", "name": "L", "type": "raster", "visible": true,
                "opacity": "not-a-number", "locked": false, "blendMode": "normal",
                "transform": {"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0,"flipH":false,"flipV":false},
                "width": 10, "height": 10
            }]
        });

        let err = prepare_save(target.to_str().unwrap(), &bad, &trusted)
            .expect_err("bad document must be rejected");
        assert_eq!(
            err["error"]["code"],
            Value::String("E_VALIDATION".to_string())
        );

        assert!(
            !target.exists(),
            "a rejected save must not create the target file"
        );
        assert!(!tmp.exists(), "a rejected save must not leave a temp file");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A live streaming session, built the way `save_project_streaming_begin`
    /// builds it, so `end_session` can be driven end to end.
    fn start_session(
        dir: &std::path::Path,
        target: &std::path::Path,
        document_json: &str,
    ) -> (StreamingSaveState, String) {
        let tmp_path = dir.join("out.ptz.tmp");
        let file = std::fs::File::create(&tmp_path).expect("temp file");
        let mut zip = zip::ZipWriter::new(file);
        write_document_json_into_zip(&mut zip, document_json).expect("document.json writes");
        let mut sessions = HashMap::new();
        sessions.insert(
            "h1".to_string(),
            StreamingSaveSession {
                tmp_path,
                final_path: target.to_path_buf(),
                zip: Some(zip),
                created_at: Instant::now(),
            },
        );
        (
            StreamingSaveState {
                sessions: Mutex::new(sessions),
            },
            "h1".to_string(),
        )
    }

    /// BLOCKING: a finalize failure must not strand a temp file. Before the fix
    /// the session was removed BEFORE `zip.finish()` / `sync_all` / `rename`, so
    /// each of those early-returned with no handle left to clean up and a
    /// `<name>.ptz.tmp` was stranded permanently.
    #[test]
    fn a_finalize_failure_leaves_no_temp_file_and_keeps_the_session() {
        let dir = std::env::temp_dir().join("photrez_finalize_fail_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");

        // Make the rename fail: the target is a DIRECTORY, and a file cannot be
        // renamed over one.
        let target = dir.join("out.ptz");
        std::fs::create_dir_all(&target).expect("target is a directory");

        let (state, handle) = start_session(&dir, &target, "{}");
        let tmp_path = dir.join("out.ptz.tmp");
        assert!(tmp_path.exists(), "precondition: temp file exists mid-save");

        let mut sessions = state.sessions.lock().unwrap();
        let err = end_session(&mut sessions, &handle).expect_err("rename onto a dir must fail");
        assert_eq!(err["error"]["code"], Value::String("E_IO".to_string()));

        assert!(
            !tmp_path.exists(),
            "a finalize failure must not leave a .ptz.tmp behind"
        );
        assert!(
            sessions.contains_key(&handle),
            "the session is retained so the host's cancel can still resolve"
        );

        drop(sessions);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The happy path still replaces the target atomically and cleans up.
    #[test]
    fn a_successful_finalize_renames_and_clears_the_session() {
        let dir = std::env::temp_dir().join("photrez_finalize_ok_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");

        let target = dir.join("out.ptz");
        let (state, handle) = start_session(&dir, &target, r#"{"id":"d"}"#);
        let tmp_path = dir.join("out.ptz.tmp");

        let mut sessions = state.sessions.lock().unwrap();
        let final_path = end_session(&mut sessions, &handle).expect("finalize succeeds");

        assert_eq!(final_path, target);
        assert!(target.exists(), "target replaced");
        assert!(!tmp_path.exists(), "temp file gone after rename");
        assert!(
            !sessions.contains_key(&handle),
            "session cleared on success"
        );

        drop(sessions);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A second `end` on a finished handle must still be rejected cleanly.
    #[test]
    fn a_double_end_is_rejected_after_success() {
        let dir = std::env::temp_dir().join("photrez_double_end_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");

        let target = dir.join("out.ptz");
        let (state, handle) = start_session(&dir, &target, r#"{"id":"d"}"#);
        let mut sessions = state.sessions.lock().unwrap();
        end_session(&mut sessions, &handle).expect("first end succeeds");
        let err = end_session(&mut sessions, &handle).expect_err("second end must fail");
        assert_eq!(
            err["error"]["code"],
            Value::String("E_VALIDATION".to_string())
        );

        drop(sessions);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The guard above must be load-bearing: a well-formed document passes the
    /// same step, so the rejection above is the serialization doing the work
    /// and not the function failing for every input.
    #[test]
    fn a_good_document_passes_the_pre_filesystem_step() {
        let dir = std::env::temp_dir().join("photrez_prepare_save_ok_test");
        let target = dir.join("out.ptz");
        let trusted = trusted_for(&dir, &target);

        let host_document: Value = serde_json::from_str(REAL_DUMPED_MODEL).unwrap();
        let (safe_path, document_json) =
            prepare_save(target.to_str().unwrap(), &host_document, &trusted).expect("valid save");
        assert!(safe_path.to_string_lossy().contains("out.ptz"));

        let reread =
            PtzDocument::from_json(&document_json).expect("serialized payload is readable");
        assert_eq!(reread.layers.len(), 2);
        assert_eq!(reread.viewport.zoom, 2.6840000406901043);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
