// Native (Tauri) authority command surface for the per-document
// `ProtocolEngine`. ADDITIVE ONLY - mirrors the wasm `protocol_*` bridge in
// `document_core.rs` but drives the REAL `pixel_store` REGISTRY (keyed by
// `doc_id`) instead of the wasm `ENGINES` thread_local. The facade dispatches
// here by default (native authority); set `photrez.facadeAuthority` to `wasm`
// to route to the wasm engine. Production behavior follows the flag.
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

/// Native read-only layer-id probe for the native `ProtocolEngine` authority.
///
/// Returns the per-doc native `ProtocolEngine`'s layer-id set as JSON (a bare
/// array of id strings) WITHOUT serializing the full snapshot - the guarded
/// metadata funnels only need membership, never values. There is NO wasm
/// version export: the bridge parses the snapshot instead (same stance as the
/// version probe). Reuses the same REGISTRY access the sibling native commands
/// use (`registry()` -> `docs[doc].history`). Same missing-doc error stance as
/// the siblings (`"document not open: {key}"`); native is the single canonical
/// authority and reports a missing doc as an error. Same `""->"default"`
/// normalization.
#[tauri::command]
pub fn protocol_layer_ids_native(doc_id: String) -> Result<String, String> {
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
    let ids: Vec<&str> = snap.layers.iter().map(|l| l.id.as_str()).collect();
    Ok(serde_json::to_string(&ids).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint_parity_cmds::TEST_REGISTRY_LOCK;
    use photrez_core::pixel_store::registry;

    const SNAP_DOC: &str = "snapshot-native-test-doc";
    const VERSION_DOC: &str = "version-native-test-doc";

    pub(super) fn open_doc(key: &str) {
        let mut g = registry();
        let reg = g.get_or_insert_with(Default::default);
        reg.open_document(key);
    }

    pub(super) fn close_doc(key: &str) {
        let mut g = registry();
        if let Some(reg) = g.as_mut() {
            reg.close_document(key);
        }
    }

    #[test]
    fn snapshot_native_returns_seeded_engine() {
        let _registry_guard = TEST_REGISTRY_LOCK.lock().unwrap();
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
        let _registry_guard = TEST_REGISTRY_LOCK.lock().unwrap();
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

    const IDS_DOC: &str = "layer-ids-native-test-doc";

    #[test]
    fn layer_ids_native_returns_seeded_ids_only() {
        let _registry_guard = TEST_REGISTRY_LOCK.lock().unwrap();
        open_doc(IDS_DOC);
        // An empty doc probes to an empty id set with no snapshot serialization.
        let empty = protocol_layer_ids_native(IDS_DOC.to_string()).expect("empty ids ok");
        assert_eq!(empty, "[]");
        // Seed two layers at version 3 through the sibling native seed command.
        let payload = r#"{"version":3,"layers":[{"id":"L1","name":"Base","visible":true,"opacity":1.0,"resourceId":1,"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0},{"id":"L2","name":"Top","visible":false,"opacity":0.5,"resourceId":2,"x":1,"y":2,"scaleX":1,"scaleY":1,"rotation":0}]}"#;
        let seed = protocol_seed_native(payload.to_string(), IDS_DOC.to_string());
        assert!(seed.is_ok(), "seed failed: {:?}", seed.err());

        let out = protocol_layer_ids_native(IDS_DOC.to_string()).expect("ids ok");
        let parsed: Vec<String> = serde_json::from_str(&out).expect("valid json");
        assert_eq!(parsed, vec!["L1".to_string(), "L2".to_string()]);
        close_doc(IDS_DOC);
    }

    #[test]
    fn layer_ids_native_missing_doc_errors() {
        // Same shared-registry caveat as snapshot_native_missing_doc_errors: do
        // not mutate the global `registry()` here (it races the parallel suite).
        // Assert a valid rejection envelope - either missing-doc stance is a
        // correct rejection of a missing doc regardless of execution order.
        let missing = "layer-ids-native-missing-doc-NOPE";
        let out = protocol_layer_ids_native(missing.to_string());
        assert!(out.is_err(), "missing doc must error");
        let err = out.unwrap_err();
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
        let _registry_guard = TEST_REGISTRY_LOCK.lock().unwrap();
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
        let _registry_guard = TEST_REGISTRY_LOCK.lock().unwrap();
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
        let _registry_guard = TEST_REGISTRY_LOCK.lock().unwrap();
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

    // ── Native command surface: full routed sequence, undo/redo, error
    // envelope shape, wrapper transparency, canonical read-back ─────────────
    //
    // These tests drive the real native command functions (the ones the TS
    // client reaches through raw `invoke()`), so the whole desktop surface is
    // exercised headlessly: serde envelope parse, registry lookup, engine
    // apply, `CommandResult` serialize, and the `"CODE: message"` rejection
    // string. Each test owns a unique doc key AND holds TEST_REGISTRY_LOCK:
    // unique keys alone do not protect against a sibling test wiping the
    // whole process-global registry with reset(), so registry-touching tests
    // run one at a time like the paint_parity and document_snapshot suites.

    use photrez_core::protocol::CONTRACT_VERSION;

    const SEQ_DOC: &str = "apply-sequence-native-test-doc";
    const UNDO_DOC: &str = "undo-redo-native-test-doc";
    const ERR_INVALID_DOC: &str = "error-invalid-native-test-doc";
    const ERR_VERSION_DOC: &str = "error-version-native-test-doc";
    const PARITY_CMD_DOC: &str = "parity-command-native-test-doc";
    const PARITY_DIRECT_DOC: &str = "parity-direct-native-test-doc";
    const CANON_READ_DOC: &str = "canonical-read-native-test-doc";

    /// Real-shape `TextData` payload used by the typed-add / params arms. The
    /// field names are camelCase because `TextData` is a plain struct (the
    /// snake_case rule applies to the `Command` enum's struct-variant fields).
    const SEQ_TEXT_DATA: &str = r##"{"content":"Hi","fontFamily":"Arial","fontSize":32.0,"fontWeight":700.0,"fontStyle":"italic","color":"#000000","align":"center","lineHeight":1.2,"letterSpacing":0.0,"boxMode":"area","boxWidth":300.0,"boxHeight":60.0,"stroke":{"width":2.0,"color":"#FF0000","align":"outside"},"underline":false,"strikethrough":false,"uppercase":true}"##;

    /// A three-layer canonical document matching the render layers seeded by
    /// `seed_routed_doc` (so the canonical up-projection preserves engine
    /// resource ids and order). The top layer is typed `text` so the
    /// rasterize arm exercises a real parametric-to-raster transition.
    pub(super) fn sequence_canonical_fixture() -> String {
        format!(
            r#"{{"id":"seq-doc","name":"Seq","width":800.0,"height":600.0,"layers":[
                {{"id":"l-top","name":"Top","type":"text","visible":true,"opacity":1.0,"locked":false,"blendMode":"normal","transform":{{"x":0.0,"y":0.0,"scaleX":1.0,"scaleY":1.0,"rotation":0.0,"flipH":false,"flipV":false}},"width":300.0,"height":60.0,"textData":{text}}},
                {{"id":"l-mid","name":"Mid","type":"raster","visible":true,"opacity":1.0,"locked":false,"blendMode":"normal","transform":{{"x":0.0,"y":0.0,"scaleX":1.0,"scaleY":1.0,"rotation":0.0,"flipH":false,"flipV":false}},"width":200.0,"height":150.0}},
                {{"id":"l-bot","name":"Bot","type":"raster","visible":true,"opacity":1.0,"locked":false,"blendMode":"normal","transform":{{"x":0.0,"y":0.0,"scaleX":1.0,"scaleY":1.0,"rotation":0.0,"flipH":false,"flipV":false}},"width":200.0,"height":150.0}}
            ]}}"#,
            text = SEQ_TEXT_DATA
        )
    }

    /// The initial layer load pushed by `seed_routed_doc`. Exposed so the
    /// engine-identity reference can start from byte-identical engine state
    /// instead of restating the same layer set.
    pub(super) const ROUTED_SEED_LAYERS: &str = r#"[{"id":"l-top","name":"Top","visible":true,"opacity":1.0,"resourceId":1,"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0},{"id":"l-mid","name":"Mid","visible":true,"opacity":1.0,"resourceId":2,"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0},{"id":"l-bot","name":"Bot","visible":true,"opacity":1.0,"resourceId":3,"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0}]"#;

    /// Open `doc` and seed it through the native seed commands: the initial
    /// layer load (`protocol_seed_native`) followed by the canonical shadow
    /// push (`protocol_seed_canonical_native`), which also establishes the
    /// document dimensions the selection and merge arms read.
    pub(super) fn seed_routed_doc(doc: &str) {
        open_doc(doc);
        let payload = format!(r#"{{"version":10,"layers":{}}}"#, ROUTED_SEED_LAYERS);
        protocol_seed_native(payload, doc.to_string()).expect("seed layers");
        protocol_seed_canonical_native(sequence_canonical_fixture(), doc.to_string())
            .expect("seed canonical");
    }

    /// Build an envelope JSON string with an explicit expected version. The
    /// command struct-variant fields stay snake_case (see the `Command` enum's
    /// serde contract); only the envelope struct fields are camelCase.
    pub(super) fn envelope_json(command: &str, expected_version: u64) -> String {
        format!(
            r#"{{"contractVersion":{cv},"expectedVersion":{ev},"command":{cmd}}}"#,
            cv = CONTRACT_VERSION,
            ev = expected_version,
            cmd = command
        )
    }

    /// Stable digest of the observable engine state: layers in engine order,
    /// selection, and document dimensions. Version and the per-layer
    /// `resourceId` / `dirtyRect` are excluded because the version counter
    /// changes on every accepted undo/redo and the resource/dirty values are
    /// engine-internal bookkeeping, not canonical state.
    pub(super) fn snapshot_digest(snapshot: &RenderSnapshot) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut value = serde_json::to_value(snapshot).expect("snapshot serializes");
        if let Some(map) = value.as_object_mut() {
            map.remove("version");
        }
        if let Some(layers) = value.get_mut("layers").and_then(|l| l.as_array_mut()) {
            for layer in layers.iter_mut() {
                if let Some(obj) = layer.as_object_mut() {
                    obj.remove("resourceId");
                    obj.remove("dirtyRect");
                }
            }
        }
        let canonical = serde_json::to_string(&value).expect("normalized snapshot serializes");
        let mut hasher = DefaultHasher::new();
        canonical.hash(&mut hasher);
        hasher.finish()
    }

    /// Read the engine state digest through the native snapshot command.
    pub(super) fn command_digest(doc: &str) -> u64 {
        let json = protocol_snapshot_native(doc.to_string()).expect("snapshot ok");
        let snapshot: RenderSnapshot = serde_json::from_str(&json).expect("snapshot parses");
        snapshot_digest(&snapshot)
    }

    /// Apply one command JSON through the native command and assert the
    /// version advanced by exactly one. Returns the new version.
    pub(super) fn apply_ok(doc: &str, command: &str) -> u64 {
        let version = protocol_version_native(doc.to_string()).expect("version");
        let result =
            protocol_apply_command_native(envelope_json(command, version), doc.to_string())
                .unwrap_or_else(|e| panic!("apply {command} failed: {e}"));
        let parsed: CommandResult = serde_json::from_str(&result).expect("CommandResult parses");
        assert_eq!(
            parsed.document_version,
            version + 1,
            "accepted command must bump the document version by exactly one",
        );
        parsed.document_version
    }

    /// The full routed command family, one envelope body per command. Layer
    /// ids stay valid as the stack evolves. Each entry is exactly the
    /// snake_case wire shape the TS bridge produces.
    pub(super) fn routed_command_script() -> Vec<String> {
        let text = SEQ_TEXT_DATA;
        vec![
            // Layer add / transform / metadata.
            r#"{"type":"addLayer","id":"n1","name":"New","width":100.0,"height":80.0,"index":1}"#.to_string(),
            r#"{"type":"transformLayer","id":"n1","transform":{"x":12.0,"y":8.0,"scaleX":2.0,"scaleY":2.0,"rotation":45.0,"flipH":true,"flipV":false}}"#.to_string(),
            r#"{"type":"setOpacity","id":"n1","opacity":0.5}"#.to_string(),
            r#"{"type":"setVisible","id":"n1","visible":false}"#.to_string(),
            r#"{"type":"setLocked","id":"n1","kind":"base","locked":true}"#.to_string(),
            r#"{"type":"rename","id":"n1","name":"Renamed"}"#.to_string(),
            r#"{"type":"reorder","id":"n1","to":3}"#.to_string(),
            r#"{"type":"setBackgroundFlag","id":"l-bot"}"#.to_string(),
            r#"{"type":"setBlendMode","id":"n1","mode":"multiply"}"#.to_string(),
            format!(r#"{{"type":"setLayerParams","id":"l-top","text_data":{text}}}"#),
            r#"{"type":"setAdjustment","id":"l-mid","adjustment":{"brightness":15.0,"contrast":-10.0,"saturation":0.0}}"#.to_string(),
            // Structural.
            r#"{"type":"duplicateLayer","id":"l-mid","new_id":"n1-dup"}"#.to_string(),
            r#"{"type":"mergeDown","id":"n1-dup","merged_id":"m1"}"#.to_string(),
            r#"{"type":"rasterizeLayer","id":"l-top"}"#.to_string(),
            r#"{"type":"mergeSelected","ids":["m1","n1"],"merged_id":"m2"}"#.to_string(),
            format!(r#"{{"type":"addLayer","id":"t1","name":"Typed","width":200.0,"height":60.0,"index":0,"layer_type":"text","text_data":{text}}}"#),
            format!(r#"{{"type":"setLayerParams","id":"t1","text_data":{text}}}"#),
            r#"{"type":"rasterizeLayer","id":"t1"}"#.to_string(),
            r#"{"type":"flatten","merged_id":"flat1"}"#.to_string(),
            r#"{"type":"addLayer","id":"post","name":"Post","width":10.0,"height":10.0,"index":0}"#.to_string(),
            r#"{"type":"duplicateLayer","id":"post","new_id":"post-dup"}"#.to_string(),
            r#"{"type":"deleteLayer","id":"post-dup"}"#.to_string(),
            // Canvas size.
            r#"{"type":"resizeCanvas","width":1024.0,"height":768.0}"#.to_string(),
            r#"{"type":"cropCanvas","x":0.0,"y":0.0,"width":512.0,"height":512.0}"#.to_string(),
            r#"{"type":"applyCrop","x":0.0,"y":0.0,"width":512.0,"height":512.0,"rotation":0.0,"target_width":256.0,"target_height":256.0}"#.to_string(),
            // Selection.
            r#"{"type":"setSelection","selection":{"x":1.0,"y":2.0,"width":50.0,"height":40.0,"angle":0.0,"shape":"rect","inverted":false}}"#.to_string(),
            r#"{"type":"clearSelection"}"#.to_string(),
            r#"{"type":"selectAll"}"#.to_string(),
            r#"{"type":"invertSelection"}"#.to_string(),
        ]
    }

    /// Run the shared script by issuing each command through the native apply
    /// command (the exact code path the desktop client uses).
    fn run_script_through_commands(doc: &str) -> (u64, u64) {
        for command in routed_command_script() {
            let version = protocol_version_native(doc.to_string()).expect("version");
            protocol_apply_command_native(envelope_json(&command, version), doc.to_string())
                .unwrap_or_else(|e| panic!("command path failed at {command}: {e}"));
        }
        let version = protocol_version_native(doc.to_string()).expect("version");
        (command_digest(doc), version)
    }

    /// Run the same script by deserializing each envelope in the test and
    /// calling `engine.history.apply` directly, bypassing the command wrapper.
    fn run_script_through_engine(doc: &str) -> (u64, u64) {
        for command in routed_command_script() {
            let mut guard = registry();
            let reg = guard.as_mut().expect("registry initialized");
            let engine = reg.docs.get_mut(doc).expect("doc open");
            let version = engine.history.version();
            let envelope: CommandEnvelope =
                serde_json::from_str(&envelope_json(&command, version)).expect("envelope parses");
            engine.history.apply(envelope).expect("direct apply ok");
        }
        let guard = registry();
        let reg = guard.as_ref().expect("registry initialized");
        let engine = reg.docs.get(doc).expect("doc open");
        (
            snapshot_digest(&engine.history.snapshot()),
            engine.history.version(),
        )
    }

    /// Every routed command is accepted through the native command surface, the
    /// version advances by exactly one per accepted command, the returned
    /// `CommandResult` reports that version, and the snapshot read-back stays
    /// parseable across the whole sequence.
    #[test]
    fn native_apply_command_drives_full_routed_sequence() {
        let _registry_guard = TEST_REGISTRY_LOCK.lock().unwrap();
        seed_routed_doc(SEQ_DOC);
        let seed_digest = command_digest(SEQ_DOC);
        let mut version = protocol_version_native(SEQ_DOC.to_string()).expect("version");
        assert_eq!(
            version, 10,
            "seeded version must be the seed payload version"
        );

        let script = routed_command_script();
        assert!(
            script.len() >= 28,
            "script must cover the full routed command family, got {}",
            script.len()
        );

        for (step, command) in script.iter().enumerate() {
            let result =
                protocol_apply_command_native(envelope_json(command, version), SEQ_DOC.to_string())
                    .unwrap_or_else(|e| panic!("step {step} ({command}) failed: {e}"));
            let parsed: CommandResult =
                serde_json::from_str(&result).expect("CommandResult parses");
            let next = protocol_version_native(SEQ_DOC.to_string()).expect("version");
            assert_eq!(
                next,
                version + 1,
                "step {step} ({command}) must advance the version by exactly one"
            );
            assert_eq!(
                parsed.document_version, next,
                "step {step} ({command}) CommandResult must report the post-apply version"
            );
            version = next;
        }

        let final_digest = command_digest(SEQ_DOC);
        assert_ne!(
            final_digest, seed_digest,
            "the script must have changed the observable state"
        );
        close_doc(SEQ_DOC);
    }

    /// Undo/redo through the native command surface restores the exact
    /// pre-transition snapshot at each cursor position. Undo past the bottom
    /// and redo past the top are accepted no-ops that still advance the
    /// version, matching the engine's real behavior.
    #[test]
    fn native_apply_command_undo_redo_round_trip() {
        let _registry_guard = TEST_REGISTRY_LOCK.lock().unwrap();
        seed_routed_doc(UNDO_DOC);
        let seed_digest = command_digest(UNDO_DOC);

        apply_ok(
            UNDO_DOC,
            r#"{"type":"addLayer","id":"u1","name":"U","width":10.0,"height":10.0,"index":0}"#,
        );
        let after_add = command_digest(UNDO_DOC);
        assert_ne!(after_add, seed_digest, "add must change the state");

        apply_ok(
            UNDO_DOC,
            r#"{"type":"setOpacity","id":"u1","opacity":0.25}"#,
        );
        let after_opacity = command_digest(UNDO_DOC);
        assert_ne!(after_opacity, after_add, "opacity must change the state");

        apply_ok(UNDO_DOC, r#"{"type":"undo"}"#);
        assert_eq!(
            command_digest(UNDO_DOC),
            after_add,
            "undo restores the prior cursor"
        );
        apply_ok(UNDO_DOC, r#"{"type":"redo"}"#);
        assert_eq!(
            command_digest(UNDO_DOC),
            after_opacity,
            "redo restores the later cursor"
        );

        apply_ok(UNDO_DOC, r#"{"type":"undo"}"#);
        apply_ok(UNDO_DOC, r#"{"type":"undo"}"#);
        assert_eq!(
            command_digest(UNDO_DOC),
            seed_digest,
            "two undos reach the seeded state"
        );

        // Undo past the bottom: the real engine returns an accepted no-op that
        // still bumps the version (it does not error).
        let before = apply_ok(UNDO_DOC, r#"{"type":"undo"}"#);
        assert_eq!(
            before,
            protocol_version_native(UNDO_DOC.to_string()).expect("version")
        );
        assert_eq!(
            command_digest(UNDO_DOC),
            seed_digest,
            "bottom no-op changes no state"
        );

        // Redo twice returns to the top; a third redo is the top no-op.
        apply_ok(UNDO_DOC, r#"{"type":"redo"}"#);
        apply_ok(UNDO_DOC, r#"{"type":"redo"}"#);
        assert_eq!(
            command_digest(UNDO_DOC),
            after_opacity,
            "two redos reach the top"
        );
        apply_ok(UNDO_DOC, r#"{"type":"redo"}"#);
        assert_eq!(
            command_digest(UNDO_DOC),
            after_opacity,
            "top no-op changes no state"
        );

        close_doc(UNDO_DOC);
    }

    /// An arm-level rejection that crosses the real JSON wire returns the bare
    /// `"CODE: message"` string (not a JSON object) and leaves the version
    /// untouched. This is the exact rejection shape the desktop client parses
    /// from a raw `invoke()`.
    #[test]
    fn native_apply_command_error_envelope_shape() {
        let _registry_guard = TEST_REGISTRY_LOCK.lock().unwrap();
        seed_routed_doc(ERR_INVALID_DOC);
        let version = protocol_version_native(ERR_INVALID_DOC.to_string()).expect("version");
        let command = r#"{"type":"setSelection","selection":{"x":0.0,"y":0.0,"width":-5.0,"height":10.0,"angle":0.0,"shape":"rect","inverted":false}}"#;
        let err = protocol_apply_command_native(
            envelope_json(command, version),
            ERR_INVALID_DOC.to_string(),
        )
        .expect_err("negative selection geometry must be rejected");
        assert!(
            err.starts_with("E_INVALID: "),
            "expected bare 'E_INVALID: message', got: {err}"
        );
        assert_eq!(
            err.split(": ").next(),
            Some("E_INVALID"),
            "code must lead the string"
        );
        assert!(
            !err.trim_start().starts_with('{'),
            "rejection must be a bare string, not a JSON envelope: {err}"
        );
        assert_eq!(
            protocol_version_native(ERR_INVALID_DOC.to_string()).expect("version"),
            version,
            "a rejected command must not advance the version"
        );
        close_doc(ERR_INVALID_DOC);
    }

    /// Envelope-level rejections also use the `"CODE: message"` shape and are
    /// decided before the registry is touched. A non-finite geometry value
    /// cannot reach the arm's E_INVALID gate over JSON: JSON has no NaN or
    /// Infinity, so such a value arrives as `null` (or an out-of-range number)
    /// and is rejected by serde as E_ENVELOPE_PARSE. The desktop client must
    /// expect that code, not E_INVALID, for non-finite input.
    #[test]
    fn native_apply_command_parse_error_envelope_shape() {
        let missing = "parse-error-native-missing-doc-NOPE";
        let err = protocol_apply_command_native("{ not json".to_string(), missing.to_string())
            .expect_err("malformed json must be rejected");
        assert!(
            err.starts_with("E_ENVELOPE_PARSE: "),
            "expected bare 'E_ENVELOPE_PARSE: message', got: {err}"
        );

        let unknown = r#"{"contractVersion":2,"command":{"type":"doesNotExist"}}"#.to_string();
        let err = protocol_apply_command_native(unknown, missing.to_string())
            .expect_err("unknown command type must be rejected");
        assert!(
            err.starts_with("E_ENVELOPE_PARSE: "),
            "unknown command type must be E_ENVELOPE_PARSE, got: {err}"
        );

        let non_finite = r#"{"contractVersion":2,"command":{"type":"setSelection","selection":{"x":null,"y":0.0,"width":10.0,"height":10.0,"angle":0.0,"shape":"rect","inverted":false}}}"#.to_string();
        let err = protocol_apply_command_native(non_finite, missing.to_string())
            .expect_err("null geometry must be rejected at parse");
        assert!(
            err.starts_with("E_ENVELOPE_PARSE: "),
            "non-finite geometry over the JSON wire must be E_ENVELOPE_PARSE, got: {err}"
        );
    }

    /// Version and contract mismatches reject with their own codes, and the
    /// document must be open before either check runs (the registry lookup
    /// precedes the engine's checks).
    #[test]
    fn native_apply_command_contract_and_expected_version_errors() {
        let _registry_guard = TEST_REGISTRY_LOCK.lock().unwrap();
        open_doc(ERR_VERSION_DOC);
        protocol_seed_native(
            r#"{"version":5,"layers":[{"id":"v1","name":"V","visible":true,"opacity":1.0,"resourceId":1,"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0}]}"#.to_string(),
            ERR_VERSION_DOC.to_string(),
        )
        .expect("seed");

        let contract = r#"{"contractVersion":1,"command":{"type":"noop"}}"#.to_string();
        let err = protocol_apply_command_native(contract, ERR_VERSION_DOC.to_string())
            .expect_err("contract version mismatch must be rejected");
        assert!(
            err.starts_with("E_CONTRACT_VERSION: "),
            "expected bare 'E_CONTRACT_VERSION: message', got: {err}"
        );

        let stale = envelope_json(r#"{"type":"noop"}"#, 999);
        let err = protocol_apply_command_native(stale, ERR_VERSION_DOC.to_string())
            .expect_err("stale expected version must be rejected");
        assert!(
            err.starts_with("E_VERSION_MISMATCH: "),
            "expected bare 'E_VERSION_MISMATCH: message', got: {err}"
        );
        close_doc(ERR_VERSION_DOC);
    }

    /// The command wrapper is transparent to the engine: the same script run
    /// through the native command and directly through `engine.history.apply`
    /// must reach the identical final state digest and version. This is the
    /// native-vs-engine parity check (the wrapper adds only serde parse,
    /// registry lookup, result serialize, and error formatting).
    #[test]
    fn native_apply_command_wrapper_is_transparent_to_engine() {
        let _registry_guard = TEST_REGISTRY_LOCK.lock().unwrap();
        seed_routed_doc(PARITY_CMD_DOC);
        seed_routed_doc(PARITY_DIRECT_DOC);
        assert_eq!(
            command_digest(PARITY_CMD_DOC),
            command_digest(PARITY_DIRECT_DOC),
            "both docs must start from the identical seeded state"
        );

        let (command_digest_final, command_version) = run_script_through_commands(PARITY_CMD_DOC);
        let (direct_digest_final, direct_version) = run_script_through_engine(PARITY_DIRECT_DOC);

        assert_eq!(
            command_version, direct_version,
            "the wrapper must not alter version bookkeeping"
        );
        assert_eq!(
            command_digest_final, direct_digest_final,
            "the wrapper must be transparent to the engine state"
        );

        close_doc(PARITY_CMD_DOC);
        close_doc(PARITY_DIRECT_DOC);
    }

    /// `protocol_canonical_native` is exercised as a real consumer: after a
    /// canvas resize and a selection change it returns a parseable canonical
    /// document reflecting the applied dimensions and selection.
    #[test]
    fn native_canonical_read_back_reflects_applied_state() {
        let _registry_guard = TEST_REGISTRY_LOCK.lock().unwrap();
        seed_routed_doc(CANON_READ_DOC);
        apply_ok(
            CANON_READ_DOC,
            r#"{"type":"resizeCanvas","width":1024.0,"height":768.0}"#,
        );
        apply_ok(
            CANON_READ_DOC,
            r#"{"type":"setSelection","selection":{"x":3.0,"y":4.0,"width":50.0,"height":40.0,"angle":0.0,"shape":"rect","inverted":false}}"#,
        );

        let read = protocol_canonical_native(CANON_READ_DOC.to_string()).expect("read-back ok");
        let parsed: serde_json::Value = serde_json::from_str(&read).expect("canonical parses");
        assert_eq!(parsed["id"], "seq-doc");
        assert_eq!(parsed["width"].as_f64(), Some(1024.0));
        assert_eq!(parsed["height"].as_f64(), Some(768.0));
        assert_eq!(parsed["selection"]["x"].as_f64(), Some(3.0));
        assert_eq!(parsed["selection"]["width"].as_f64(), Some(50.0));
        assert!(
            parsed["layers"].is_array(),
            "canonical read-back must carry layers"
        );
        close_doc(CANON_READ_DOC);
    }

    /// A missing doc rejects the canonical read-back cleanly (same
    /// missing-doc envelope as the sibling read commands) without mutating the
    /// shared registry, so this stays order-independent in the parallel suite.
    #[test]
    fn native_canonical_read_back_missing_doc_errors_cleanly() {
        let res = protocol_canonical_native("canonical-read-native-missing-doc-NOPE".to_string());
        assert!(res.is_err(), "missing doc must error");
        let err = res.unwrap_err();
        assert!(
            err.starts_with("document not open:") || err.starts_with("pixel store not initialized"),
            "error must be a valid missing-doc rejection envelope, got: {err}"
        );
    }

    /// Measurement bench (the NUMBER is the point, not correctness): times the
    /// in-process cost of one commit through the FULL native command path -
    /// `serde_json` envelope parse -> registry lookup -> `history.apply` ->
    /// `CommandResult` serialize. This is the per-commit compute+serde cost the
    /// native authority adds per commit.
    ///
    /// It EXCLUDES the Tauri webview<->process IPC transport, which cannot be
    /// measured headlessly; the live per-commit cost is this number plus that
    /// hop. The assertion is a deliberately lenient ceiling so the bench
    /// documents the figure instead of flaking on a slow CI runner.
    #[test]
    fn native_commit_latency_bench() {
        use std::time::{Duration, Instant};

        let _registry_guard = TEST_REGISTRY_LOCK.lock().unwrap();
        const BENCH_DOC: &str = "native-commit-latency-bench-doc";
        const ITERS: usize = 1000;
        const BATCH: usize = 100;
        /// Lenient in-process ceiling: 2ms per commit. The real figure is tens
        /// of microseconds; this only catches a pathological regression.
        const BUDGET_US: f64 = 2000.0;

        seed_routed_doc(BENCH_DOC);
        let mut version = protocol_version_native(BENCH_DOC.to_string()).expect("version");

        // Two realistic small commit-boundary envelopes: a flat metadata op and
        // a small nested-struct op, both on the same raster layer. Alternating
        // keeps each accepted command advancing the document version.
        let opacity = r#"{"type":"setOpacity","id":"l-mid","opacity":0.5}"#;
        let adjustment = r#"{"type":"setAdjustment","id":"l-mid","adjustment":{"brightness":15.0,"contrast":-10.0,"saturation":0.0}}"#;

        let mut per_op_us: Vec<f64> = Vec::with_capacity(ITERS);
        let mut opacity_ops = 0usize;
        let mut adjustment_ops = 0usize;

        for i in 0..ITERS {
            let is_opacity = i % 2 == 0;
            let command = if is_opacity { opacity } else { adjustment };
            // Envelope construction is client-side; keep it out of the timed
            // region so we measure the native command path, not `format!`.
            let envelope = envelope_json(command, version);

            let t0 = Instant::now();
            let result = protocol_apply_command_native(envelope, BENCH_DOC.to_string())
                .expect("bench command accepted");
            let elapsed = t0.elapsed();
            per_op_us.push(elapsed.as_secs_f64() * 1e6);

            let parsed: CommandResult = serde_json::from_str(&result).expect("result parses");
            assert_eq!(
                parsed.document_version,
                version + 1,
                "each accepted bench commit must bump the version by one"
            );
            version = parsed.document_version;
            if is_opacity {
                opacity_ops += 1;
            } else {
                adjustment_ops += 1;
            }
        }

        // Serde-only control: parse the envelope and serialize it back, with no
        // engine apply, so the report separates serde-in/out from compute.
        let serde_sample = envelope_json(opacity, 0);
        let mut serde_total = Duration::ZERO;
        for _ in 0..ITERS {
            let t0 = Instant::now();
            let env: CommandEnvelope =
                serde_json::from_str(&serde_sample).expect("envelope parses");
            let _ = serde_json::to_string(&env).expect("envelope serializes");
            serde_total += t0.elapsed();
        }

        let total_us: f64 = per_op_us.iter().sum();
        let mean_us = total_us / ITERS as f64;
        let serde_mean_us = serde_total.as_secs_f64() * 1e6 / ITERS as f64;
        let apply_mean_us = mean_us - serde_mean_us;

        // Per-op mean within each batch of 100, then the median of those batch
        // means (robust against a single scheduling hiccup).
        let mut sorted_batch_means: Vec<f64> = per_op_us
            .chunks(BATCH)
            .map(|c| c.iter().sum::<f64>() / c.len() as f64)
            .collect();
        sorted_batch_means.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
        let batch_median_us = sorted_batch_means[sorted_batch_means.len() / 2];

        // Per-command split: even indices are setOpacity, odd are setAdjustment.
        let per_command = |us: &[f64]| -> (f64, f64) {
            let mean = us.iter().sum::<f64>() / us.len() as f64;
            let mut sorted = us.to_vec();
            sorted.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
            (mean, sorted[sorted.len() / 2])
        };
        let opacity_us: Vec<f64> = per_op_us.iter().step_by(2).copied().collect();
        let adjustment_us: Vec<f64> = per_op_us.iter().skip(1).step_by(2).copied().collect();
        let (opacity_mean, opacity_median) = per_command(&opacity_us);
        let (adjustment_mean, adjustment_median) = per_command(&adjustment_us);

        assert_eq!(opacity_ops, ITERS / 2);
        assert_eq!(adjustment_ops, ITERS / 2);

        eprintln!(
            "native_commit_latency_bench: N={ITERS} (setOpacity={opacity_ops}, setAdjustment={adjustment_ops})"
        );
        eprintln!(
            "  per-op total (serde-in + apply + serde-out): mean={mean_us:.2}us batch-median={batch_median_us:.2}us"
        );
        eprintln!("  setOpacity:    mean={opacity_mean:.2}us median={opacity_median:.2}us");
        eprintln!("  setAdjustment: mean={adjustment_mean:.2}us median={adjustment_median:.2}us");
        eprintln!("  serde-only (envelope parse + serialize):      mean={serde_mean_us:.2}us");
        eprintln!("  derived engine.apply portion:                 mean~={apply_mean_us:.2}us");
        eprintln!(
            "  budget assert: mean {mean_us:.2}us < {BUDGET_US:.0}us -> PASS (in-process only; IPC hop excluded)"
        );

        assert!(
            mean_us < BUDGET_US,
            "in-process per-commit compute+serde mean {mean_us:.2}us exceeded the {BUDGET_US:.0}us lenient budget"
        );

        close_doc(BENCH_DOC);
    }
}

// Engine-identity attestation for the native protocol authority. Kept in a
// sibling file so the test body does not grow this module further; it reuses the
// routed script and digest helpers declared above instead of restating them.
#[cfg(test)]
#[path = "protocol_native_engine_identity_tests.rs"]
mod engine_identity_tests;
