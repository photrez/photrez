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

use photrez_core::canonical_model::CanonicalDocument;
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

/// Seed a complete `CanonicalDocument` copy into the native per-doc
/// `ProtocolEngine`, alongside the RenderLayer set the sibling `protocol_seed_native`
/// seeds. ADDITIVE authority command (native-canonical-authority handoff plumbing):
/// nothing routes through it yet, so production behavior is unchanged. Mirrors the
/// sibling native protocol commands' authority stance -- the doc MUST already be
/// open; a missing doc is an ERROR.
///
/// Unlike `protocol_seed_native` (only-when-empty), the canonical copy is a SHADOW
/// of the authoritative TS push and is replaced unconditionally -- a re-open must
/// refresh it. Returns the same `"null"` ack as `protocol_register_adapter_native`
/// so the TS client can `JSON.parse` the register / seed-canonical pair
/// uniformly. Malformed JSON is rejected with `E_CANONICAL_PARSE`.
#[tauri::command]
pub fn protocol_seed_canonical_native(
    payload_json: String,
    doc_id: String,
) -> Result<String, String> {
    let doc: CanonicalDocument =
        serde_json::from_str(&payload_json).map_err(|e| format!("E_CANONICAL_PARSE: {e}"))?;
    let doc_key = resolve_doc_key(&doc_id).to_string();
    let mut reg_guard = registry();
    let reg = reg_guard.get_or_insert_with(Default::default);
    let engine = reg
        .docs
        .get_mut(&doc_key)
        .ok_or_else(|| format!("document not open: {doc_key}"))?;
    engine.history.seed_canonical(doc);
    Ok(serde_json::to_string(&()).unwrap())
}

/// Read back the seeded `CanonicalDocument` copy for the native per-doc
/// `ProtocolEngine`. ADDITIVE / UNWIRED: no runtime path consumes the result yet.
///
/// Reuses the REGISTRY access the sibling read-only native commands use
/// (`registry()` -> `docs[doc].history`). Same missing-doc error stance as the
/// siblings (`"document not open: {key}"`, or `"pixel store not initialized"` when
/// the registry itself is not yet created). If the doc is open but was never
/// seeded with a canonical copy, rejects with `E_CANONICAL_ABSENT`.
#[tauri::command]
pub fn protocol_canonical_native(doc_id: String) -> Result<String, String> {
    let doc_key = resolve_doc_key(&doc_id).to_string();
    let reg = registry();
    let reg = reg
        .as_ref()
        .ok_or_else(|| "pixel store not initialized".to_string())?;
    let engine = reg
        .docs
        .get(&doc_key)
        .ok_or_else(|| format!("document not open: {doc_key}"))?;
    match engine.history.canonical() {
        Some(c) => Ok(serde_json::to_string(c).unwrap()),
        None => Err(format!(
            "E_CANONICAL_ABSENT: no canonical copy seeded for {doc_key}"
        )),
    }
}

/// Native mirror of wasm `protocol_snapshot_json` (`document_core.rs:774-787`).
///
/// Returns the per-doc native `ProtocolEngine`'s `RenderSnapshot` as JSON,
/// reusing the same REGISTRY access the sibling native commands use
/// (`registry()` -> `docs[doc].history`). Same missing-doc error stance as the
/// siblings (`"document not open: {key}"`); unlike the wasm `unwrap_or(empty)`
/// tolerance, native is the single canonical authority and reports a missing
/// doc as an error. Same `""->"default"` normalization. ADDITIVE only -
/// nothing routes through it yet (native-authority plumbing, not yet routed).
#[tauri::command]
pub fn protocol_snapshot_native(doc_id: String) -> Result<String, String> {
    let doc_key = resolve_doc_key(&doc_id).to_string();
    let reg = registry();
    let reg = reg
        .as_ref()
        .ok_or_else(|| "pixel store not initialized".to_string())?;
    let doc = reg
        .docs
        .get(&doc_key)
        .ok_or_else(|| format!("document not open: {doc_key}"))?;
    let snap: RenderSnapshot = doc.history.snapshot();
    Ok(serde_json::to_string(&snap).unwrap())
}

/// Native read-only version probe for the native `ProtocolEngine` authority.
///
/// Returns the per-doc native `ProtocolEngine`'s `DocumentVersion` (u64) WITHOUT
/// serializing the full snapshot - the only caller (facade `syncFromEngine`) just
/// needs the version. There is NO wasm version export: the bridge's wasm fallback
/// parses `protocol_snapshot_json` instead. Reuses the same REGISTRY access the
/// sibling native commands use (`registry()` -> `docs[doc].history`). Same
/// missing-doc error stance as the siblings (`"document not open: {key}"`); native
/// is the single canonical authority and reports a missing doc as an error. Same
/// `""->"default"` normalization.
#[tauri::command]
pub fn protocol_version_native(doc_id: String) -> Result<u64, String> {
    let doc_key = resolve_doc_key(&doc_id).to_string();
    let reg = registry();
    let reg = reg
        .as_ref()
        .ok_or_else(|| "pixel store not initialized".to_string())?;
    let doc = reg
        .docs
        .get(&doc_key)
        .ok_or_else(|| format!("document not open: {doc_key}"))?;
    Ok(doc.history.version())
}

#[cfg(test)]
mod tests {
    use super::*;
    use photrez_core::pixel_store::registry;

    const SNAP_DOC: &str = "snapshot-native-test-doc";
    const VERSION_DOC: &str = "version-native-test-doc";

    fn open_doc(key: &str) {
        let mut g = registry();
        let reg = g.get_or_insert_with(Default::default);
        reg.open_document(key);
    }

    fn close_doc(key: &str) {
        let mut g = registry();
        if let Some(reg) = g.as_mut() {
            reg.close_document(key);
        }
    }

    #[test]
    fn snapshot_native_returns_seeded_engine() {
        open_doc(SNAP_DOC);
        // Seed one layer at version 1 through the sibling native seed command.
        let payload = r#"{"version":1,"layers":[{"id":"L1","name":"Base","visible":true,"opacity":1.0,"resourceId":1,"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0}]}"#;
        let seed = protocol_seed_native(payload.to_string(), SNAP_DOC.to_string());
        assert!(seed.is_ok(), "seed failed: {:?}", seed.err());

        let snap = protocol_snapshot_native(SNAP_DOC.to_string()).expect("snapshot ok");
        let parsed: serde_json::Value = serde_json::from_str(&snap).expect("valid json");
        assert_eq!(parsed["version"], 1);
        let layers = parsed["layers"].as_array().expect("layers array");
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0]["id"], "L1");
        close_doc(SNAP_DOC);
    }

    #[test]
    fn snapshot_native_missing_doc_errors() {
        // Do NOT mutate the shared global `registry()` here: that would race
        // with the sibling `snapshot_native_returns_seeded_engine` test in the
        // parallel full suite (both share the process-global registry Mutex),
        // producing intermittent failures. Instead assert the documented
        // rejection envelope. The command rejects a missing doc with one of two
        // valid strings depending solely on global state, NOT on test order:
        //   - "document not open: {key}"  when the registry is initialized
        //   - "pixel store not initialized" when it is not
        // Both are correct rejections of a missing doc, so either proves the
        // contract without depending on execution order.
        let missing = "snapshot-native-missing-doc-NOPE";
        let snap = protocol_snapshot_native(missing.to_string());
        assert!(snap.is_err(), "missing doc must error");
        let err = snap.unwrap_err();
        assert!(
            err.starts_with("document not open:") || err.starts_with("pixel store not initialized"),
            "error must be a valid missing-doc rejection envelope, got: {err}"
        );
    }

    #[test]
    fn version_native_returns_seeded_engine_version() {
        open_doc(VERSION_DOC);
        // Seed one layer at version 7 through the sibling native seed command.
        let payload = r#"{"version":7,"layers":[{"id":"L1","name":"Base","visible":true,"opacity":1.0,"resourceId":1,"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0}]}"#;
        let seed = protocol_seed_native(payload.to_string(), VERSION_DOC.to_string());
        assert!(seed.is_ok(), "seed failed: {:?}", seed.err());
        assert_eq!(protocol_version_native(VERSION_DOC.to_string()), Ok(7));
        close_doc(VERSION_DOC);
    }

    #[test]
    fn version_native_missing_doc_errors() {
        // Same shared-registry caveat as snapshot_native_missing_doc_errors: do
        // not mutate the global `registry()` here (it races the parallel suite).
        // Assert a valid rejection envelope - either missing-doc stance is a
        // correct rejection of a missing doc regardless of execution order.
        let missing = "version-native-missing-doc-NOPE";
        let v = protocol_version_native(missing.to_string());
        assert!(v.is_err(), "missing doc must error");
        let err = v.unwrap_err();
        assert!(
            err.starts_with("document not open:") || err.starts_with("pixel store not initialized"),
            "error must be a valid missing-doc rejection envelope, got: {err}"
        );
    }

    const CANON_DOC: &str = "canonical-seed-native-test-doc";
    const CANON_BAD_DOC: &str = "canonical-bad-native-test-doc";
    const CANON_ABSENT_DOC: &str = "canonical-absent-native-test-doc";
    const CANON_MISSING_DOC: &str = "canonical-missing-native-test-doc";

    /// Real-shape `CanonicalDocument` JSON (camelCase keys) with a text layer
    /// carrying `textData`, `blendMode`, and locks - mirrors the TS builder output.
    const CANON_FIXTURE: &str = r##"{
        "id":"canon-doc-1",
        "name":"Canon",
        "width":800,
        "height":600,
        "selection":{"x":5,"y":5,"width":20,"height":15,"angle":0,"shape":"rect","inverted":false},
        "layers":[
            {
                "id":"layer-txt",
                "name":"Title",
                "type":"text",
                "visible":true,
                "opacity":1,
                "locked":true,
                "isBackground":false,
                "blendMode":"normal",
                "transform":{"x":10,"y":20,"scaleX":1,"scaleY":1,"rotation":0,"flipH":false,"flipV":false},
                "width":300,
                "height":60,
                "textData":{
                    "content":"Hi","fontFamily":"Arial","fontSize":32,"fontWeight":700,"fontStyle":"italic","color":"#000000","align":"center","lineHeight":1.2,"letterSpacing":0,"boxMode":"area","boxWidth":300,"boxHeight":60,"stroke":{"width":2,"color":"#FF0000","align":"outside"},"underline":false,"strikethrough":false,"uppercase":true
                }
            }
        ]
    }"##;

    #[test]
    fn seed_canonical_round_trips_through_read_back() {
        open_doc(CANON_DOC);
        let seed = protocol_seed_canonical_native(CANON_FIXTURE.to_string(), CANON_DOC.to_string());
        assert!(seed.is_ok(), "seed failed: {:?}", seed.err());

        let read = protocol_canonical_native(CANON_DOC.to_string()).expect("read-back ok");
        let parsed: serde_json::Value = serde_json::from_str(&read).expect("valid json");
        assert_eq!(parsed["id"], "canon-doc-1");
        assert_eq!(parsed["width"].as_f64(), Some(800.0));
        let layers = parsed["layers"].as_array().expect("layers array");
        assert_eq!(layers.len(), 1);
        let l = &layers[0];
        assert_eq!(l["type"], "text");
        assert_eq!(l["locked"], true);
        assert_eq!(l["blendMode"], "normal");
        assert_eq!(l["textData"]["content"], "Hi");
        assert_eq!(l["textData"]["fontFamily"], "Arial");
        assert_eq!(parsed["selection"]["shape"], "rect");
        close_doc(CANON_DOC);
    }

    #[test]
    fn seed_canonical_malformed_json_errors() {
        open_doc(CANON_BAD_DOC);
        let bad = "{ not canonical json";
        let res = protocol_seed_canonical_native(bad.to_string(), CANON_BAD_DOC.to_string());
        assert!(res.is_err(), "malformed json must error");
        let err = res.unwrap_err();
        assert!(
            err.starts_with("E_CANONICAL_PARSE"),
            "malformed canonical json must be E_CANONICAL_PARSE, got: {err}"
        );
        close_doc(CANON_BAD_DOC);
    }

    #[test]
    fn canonical_read_back_before_seed_errors() {
        // Same shared-registry caveat as the sibling missing-doc tests: open the
        // doc (so the registry is initialized) but never seed a canonical copy.
        open_doc(CANON_ABSENT_DOC);
        let res = protocol_canonical_native(CANON_ABSENT_DOC.to_string());
        assert!(res.is_err(), "read-back before seed must error");
        let err = res.unwrap_err();
        assert!(
            err.starts_with("E_CANONICAL_ABSENT"),
            "read-back before seed must be E_CANONICAL_ABSENT, got: {err}"
        );
        close_doc(CANON_ABSENT_DOC);
    }

    #[test]
    fn canonical_read_back_missing_doc_errors() {
        // Do NOT mutate the global `registry()` here (shared-registry race caveat):
        // assert a valid missing-doc rejection envelope, which is correct regardless
        // of whether the registry is initialized in this parallel run.
        let res = protocol_canonical_native(CANON_MISSING_DOC.to_string());
        assert!(res.is_err(), "missing doc must error");
        let err = res.unwrap_err();
        assert!(
            err.starts_with("document not open:") || err.starts_with("pixel store not initialized"),
            "error must be a valid missing-doc rejection envelope, got: {err}"
        );
    }
}
