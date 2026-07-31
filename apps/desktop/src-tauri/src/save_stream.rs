// SPDX-License-Identifier: AGPL-3.0-or-later
// --- Streaming Project Save ---
//
// save_project_streaming_* commands: incremental Zip-write sessions with
// TTL-based pruning (orphan cleanup when the frontend dies mid-save).

use serde_json::Value;
use std::collections::HashMap;
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

// ── Print Rate Limit ──

/// Minimum interval between print invocations. Guards against a compromised
/// or buggy frontend spooling an unbounded stream of print jobs (each job
/// costs a full GDI/CUPS spool and consumes paper). 2s is far below any
/// legitimate human print cadence (dialog → confirm is slower than that).
#[tauri::command]
pub(crate) fn save_project_streaming_begin(
    path: String,
    document_json: String,
    state: tauri::State<'_, StreamingSaveState>,
    trusted: tauri::State<'_, TrustedPathsState>,
) -> Result<Value, Value> {
    validate_path_extension(&path, &["ptz"], "save project")?;
    let path = validate_path_safe(&path, "save project")?;
    check_path_trusted(&trusted, &path)?;

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
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // Write document.json immediately (always small, available at start).
    zip.start_file("document.json", options)
        .map_err(|e| error_value("E_IO", &format!("Failed to start document.json: {}", e)))?;
    use std::io::Write;
    zip.write_all(document_json.as_bytes())
        .map_err(|e| error_value("E_IO", &format!("Failed to write document.json: {}", e)))?;

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
    use std::io::Write;
    zip.write_all(data).map_err(|e| {
        error_value(
            "E_IO",
            &format!("Failed to write layer {}: {}", layer_id, e),
        )
    })?;

    ok_response(serde_json::json!({ "written": layer_id }))
}

/// Finalize the streaming save — close ZIP, fsync, atomic rename.
#[tauri::command]
pub(crate) fn save_project_streaming_end(
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

    let zip = session
        .zip
        .ok_or_else(|| error_value("E_INTERNAL", "Session zip already consumed — double end?"))?;

    // Finish ZipWriter (close central directory, finalize file).
    let file = zip
        .finish()
        .map_err(|e| error_value("E_IO", &format!("Failed to finalize zip: {}", e)))?;

    // fsync data + directory for atomic durability.
    file.sync_all()
        .map_err(|e| error_value("E_IO", &format!("Failed to fsync: {}", e)))?;

    if let Some(parent) = session.final_path.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }

    // Atomic rename — either tmp replaces path completely, or the rename fails
    // and path is untouched. On Windows this is atomic if both paths are on
    // the same volume (they are, since tmp is in the same dir).
    std::fs::rename(&session.tmp_path, &session.final_path)
        .map_err(|e| error_value("E_IO", &format!("Failed to rename: {}", e)))?;

    ok_response(serde_json::json!({ "path": session.final_path }))
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
