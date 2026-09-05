// Native (Tauri) authority command surface for the per-document
// `ProtocolEngine`. ADDITIVE ONLY - mirrors the wasm `protocol_*` bridge in
// `document_core.rs` but drives the REAL `pixel_store` REGISTRY (keyed by
// `doc_id`) instead of the wasm `ENGINES` thread_local. Nothing routes through
// these commands yet (the facade stays on wasm); production behavior is unchanged.
//
// Error envelope: `Result<T, String>` where the error is a bare
// `"CODE: message"` string (the protocol contract's `ProtocolError` code plus a
// human-readable message). The 2b-3 TS client MUST call these via the raw
// `invoke()` transport (like `rust_pixels_*`); NOT the `invokeApi` envelope
// wrapper, which expects `{ ok, error }` and would mis-handle a bare-string
// rejection. (Tauri v2 `invoke()` rejects with exactly this string on `Err`.)

use photrez_core::pixel_store::registry;
use photrez_core::protocol::{
    CommandEnvelope, CommandResult, HistoryQuery, ProtocolError, RenderLayer, RenderSnapshot,
};

/// Normalize an empty/absent `doc_id` to the reserved `"default"` key so the
/// native surface and the TS client agree on the shared document. The client
/// defaults to `"default"` (matching the wasm `bridge.ts` default), so a raw
/// `""` never enters the registry as its own key - that would silently create a
/// second "default" doc disjoint from the one the client actually opened.
fn resolve_doc_key(doc_id: &str) -> &str {
    if doc_id.is_empty() {
        "default"
    } else {
        doc_id
    }
}

/// Drive the per-doc native `ProtocolEngine.apply()` through the authority
/// command. Returns the serialized `CommandResult` (same JSON the frontend
/// already parses from the wasm `protocol_apply_command` path).
///
/// A doc MUST already be open (the client opens it via `rust_pixels_open_document`
/// before issuing protocol commands). Unlike the retired wasm `or_default`
/// tolerance, a missing doc is an ERROR - native is the single canonical
/// authority (ADR 0013 D-A) and must not silently create phantom state.
#[tauri::command]
pub fn protocol_apply_command_native(
    envelope_json: String,
    doc_id: String,
) -> Result<String, String> {
    let env: CommandEnvelope =
        serde_json::from_str(&envelope_json).map_err(|e| format!("E_ENVELOPE_PARSE: {e}"))?;
    let doc_key = resolve_doc_key(&doc_id).to_string();
    let mut reg_guard = registry();
    let reg = reg_guard.get_or_insert_with(Default::default);
    let engine = reg
        .docs
        .get_mut(&doc_key)
        .ok_or_else(|| format!("document not open: {doc_key}"))?;
    engine
        .history
        .apply(env)
        .map(|r: CommandResult| serde_json::to_string(&r).unwrap())
        .map_err(|e: ProtocolError| format!("{}: {}", e.code, e.message))
}

/// Native mirror of wasm `protocol_history_query_json`.
///
/// A missing doc is an ERROR (same authority stance as the apply/commit
/// commands): the 2b-3 cache read always queries an already-open doc, so an
/// error here surfaces a real client bug rather than masking it with a phantom
/// empty history. (The loose wasm "return empty on missing" was legacy tolerance
/// being retired under D-A.)
#[tauri::command]
pub fn protocol_history_query_native(doc_id: String) -> Result<String, String> {
    let doc_key = resolve_doc_key(&doc_id).to_string();
    let reg = registry();
    let reg = reg
        .as_ref()
        .ok_or_else(|| "pixel store not initialized".to_string())?;
    let doc = reg
        .docs
        .get(&doc_key)
        .ok_or_else(|| format!("document not open: {doc_key}"))?;
    let q: HistoryQuery = doc.history.history_query();
    Ok(serde_json::to_string(&q).unwrap())
}

/// Native mirror of wasm `protocol_history_cursor_commit`.
///
/// Uses TYPED Tauri args (doc_id: String, seq: u64, direction: String) - the
/// 2b-3 TS client must pass the typed shape, NOT the wasm `json: string` envelope.
#[tauri::command]
pub fn protocol_history_cursor_commit_native(
    doc_id: String,
    seq: u64,
    direction: String,
) -> Result<String, String> {
    let doc_key = resolve_doc_key(&doc_id).to_string();
    let mut reg_guard = registry();
    let reg = reg_guard.get_or_insert_with(Default::default);
    let engine = reg
        .docs
        .get_mut(&doc_key)
        .ok_or_else(|| format!("document not open: {doc_key}"))?;
    engine
        .history
        .history_cursor_commit(seq, &direction)
        .map(|r: CommandResult| serde_json::to_string(&r).unwrap())
        .map_err(|e: ProtocolError| format!("{}: {}", e.code, e.message))
}

/// Native mirror of wasm `protocol_register_payload_adapter`.
///
/// `register_adapter` is void (infallible); return a JSON-parseable `"null"` ack
/// rather than the bare string `"ok"`, so the 2b-3 TS client can `JSON.parse` all
/// four native protocol commands uniformly. Mirrors the wasm void convention.
#[tauri::command]
pub fn protocol_register_adapter_native(
    doc_id: String,
    adapter_id: String,
) -> Result<String, String> {
    let doc_key = resolve_doc_key(&doc_id).to_string();
    let mut reg_guard = registry();
    let reg = reg_guard.get_or_insert_with(Default::default);
    let engine = reg
        .docs
        .get_mut(&doc_key)
        .ok_or_else(|| format!("document not open: {doc_key}"))?;
    engine.history.register_adapter(&adapter_id);
    Ok(serde_json::to_string(&()).unwrap())
}

/// Seed the native per-doc `ProtocolEngine` with an initial layer load.
///
/// ADDITIVE authority command (the native-canonical-engine handoff foundation):
/// nothing routes through it yet, so production behavior is unchanged. Mirrors the
/// sibling native protocol commands' authority stance — the doc MUST already be
/// open. The JSON payload carries `{ version, layers }` where each layer is a
/// full `RenderLayer` (id preserved verbatim, NO uuid minting). This is
/// initialization, not a user edit, so it creates NO history entry and no undo
/// step; the TS client's subsequent `expectedVersion` matches the seeded
/// `version` on the first real command.
///
/// Returns the serialized `RenderSnapshot` of the seeded engine, or the same
/// `"CODE: message"` error envelope as the sibling commands on failure (missing
/// doc / malformed json).
#[tauri::command]
pub fn protocol_seed_native(payload_json: String, doc_id: String) -> Result<String, String> {
    // `RenderLayer` already derives serde camelCase, so the payload's `layers`
    // deserialize directly into the engine's native layer-metadata shape.
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SeedPayload {
        version: u64,
        layers: Vec<RenderLayer>,
    }
    let payload: SeedPayload =
        serde_json::from_str(&payload_json).map_err(|e| format!("E_ENVELOPE_PARSE: {e}"))?;
    let doc_key = resolve_doc_key(&doc_id).to_string();
    let mut reg_guard = registry();
    let reg = reg_guard.get_or_insert_with(Default::default);
    let engine = reg
        .docs
        .get_mut(&doc_key)
        .ok_or_else(|| format!("document not open: {doc_key}"))?;
    engine.history.seed_layers(payload.layers, payload.version);
    let snap: RenderSnapshot = engine.history.snapshot();
    Ok(serde_json::to_string(&snap).unwrap())
}
