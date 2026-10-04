// SPDX-License-Identifier: AGPL-3.0-or-later
// Wiring proof for the External (host-handoff) metadata undo, driven through
// the REAL Tauri command layer and the REAL process-global registry.
//
// The core crate's `document_core_external_post_state_tests` proves the
// behaviour of `ProtocolEngine` directly. This file proves the production
// surface reaches it: the exact command sequence `recordExternalTransitionFor`
// issues (apps/desktop/src/lib/protocol/facadeRegistry.ts) plus the host's undo,
// with no mocks and no emulator.
//
//   1. protocol_seed_native            - the open-time layer seed
//   2. protocol_register_adapter_native- "ts-external" must be registered
//   3. protocol_seed_canonical_native  - the PRE-op push (so `before` is truthful)
//   4. protocol_apply_command_native   - RecordExternalTransition
//   5. protocol_seed_canonical_native  - the POST-op push (fills `after`)
//   6. protocol_apply_command_native   - Undo   <- the walker
//   7. protocol_history_cursor_commit_native - the host clears the barrier
//   8. protocol_apply_command_native   - Redo   <- the walker, mirrored
//
// Each step goes through the Tauri argument deserializer, so a rename or a
// shape change on any of them fails here.

use super::*;
use crate::paint_parity_cmds::TEST_REGISTRY_LOCK;

const DOC: &str = "external-metadata-undo-wiring-doc";

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

/// The `RenderLayer` payload `protocol_seed_native` deserializes: full required
/// fields, id preserved verbatim, no resource-id minting by the engine.
fn seed_payload_json(ids: &[&str]) -> String {
    let layers: Vec<String> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            format!(
                r#"{{"id":"{id}","name":"{id}","visible":true,"opacity":1.0,"resourceId":{},"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0}}"#,
                i + 1
            )
        })
        .collect();
    format!(r#"{{"version":0,"layers":[{}]}}"#, layers.join(","))
}

/// The `CanonicalDocument` payload `protocol_seed_canonical_native`
/// deserializes (camelCase, `id`/`name`/`width`/`height`/`layers`).
fn canon_payload_json(ids: &[&str]) -> String {
    let layers: Vec<String> = ids
        .iter()
        .map(|id| {
            format!(
                r#"{{"id":"{id}","name":"{id}","type":"raster","visible":true,"opacity":1,"locked":false,"blendMode":"normal","transform":{{"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0,"flipH":false,"flipV":false}},"width":10,"height":10}}"#
            )
        })
        .collect();
    format!(
        r#"{{"id":"doc","name":"doc","width":100,"height":100,"layers":[{}]}}"#,
        layers.join(",")
    )
}

fn apply(doc: &str, command_json: &str) -> serde_json::Value {
    let envelope = format!(
        r#"{{"contractVersion":{},"command":{command_json}}}"#,
        photrez_core::protocol::CONTRACT_VERSION
    );
    let out = protocol_apply_command_native(envelope, doc.to_string())
        .unwrap_or_else(|e| panic!("apply {command_json} failed: {e}"));
    serde_json::from_str(&out).expect("CommandResult json")
}

fn record_external(doc: &str, label: &str, token: &str) -> serde_json::Value {
    apply(
        doc,
        &format!(
            r#"{{"type":"recordExternalTransition","label":"{label}","affected_layer_ids":["B"],"adapter_id":"ts-external","token":"{token}","memory_cost_bytes":0,"doc_size_before":null,"doc_size_after":null,"minted_layer_ids":[]}}"#
        ),
    )
}

/// The change set as the host reads it: `RenderLayerChange` is a
/// `#[serde(tag = "kind")]` enum, so an Upsert is `{"kind":"upsert","layer":{..}}`
/// and a Remove is `{"kind":"remove","id":..,"resourceId":..}`.
fn delta_ids(changes: &serde_json::Value) -> Vec<String> {
    changes
        .as_array()
        .expect("changes array")
        .iter()
        .map(|c| match c["kind"].as_str() {
            Some("remove") => format!("-{}", c["id"].as_str().expect("remove id")),
            Some("upsert") => format!("+{}", c["layer"]["id"].as_str().expect("upsert id")),
            other => panic!("unexpected change kind {other:?}"),
        })
        .collect()
}

fn engine_layer_ids(doc: &str) -> Vec<String> {
    let out = protocol_layer_ids_native(doc.to_string()).expect("layer ids");
    let parsed: Vec<String> = serde_json::from_str(&out).expect("ids json");
    parsed
}

/// THE PRODUCTION CLAIM, EXECUTED. A host metadata delete, committed and then
/// undone, is restored by Rust alone: the undo returns a non-empty ordered
/// `RenderLayerChange` set, the engine's own layer vector changes, and the redo
/// is the mirror image. Before this file nothing pinned that at the command
/// layer - the core tests drive `ProtocolEngine`, and the facade's TS caller
/// drives these commands.
#[test]
fn external_metadata_undo_and_redo_restore_the_layer_vector_through_the_commands() {
    let _g = TEST_REGISTRY_LOCK.lock().unwrap();
    open_doc(DOC);

    // 1. Open-time layer seed: [A, B].
    protocol_seed_native(seed_payload_json(&["A", "B"]), DOC.to_string()).expect("seed");
    assert_eq!(engine_layer_ids(DOC), vec!["A", "B"]);

    // 2. The commit shim registers its adapter before every mirrored commit.
    protocol_register_adapter_native(DOC.to_string(), "ts-external".to_string()).expect("adapter");

    // 3. PRE-op push: the host's pre-mutation truth, so `before` is truthful.
    protocol_seed_canonical_native(canon_payload_json(&["A", "B"]), DOC.to_string())
        .expect("pre-op push");
    // 4. The mirrored commit.
    let rec = record_external(DOC, "Delete Layer", "tok-del");
    assert_eq!(
        rec["status"].as_str(),
        Some("external-recorded"),
        "the mirrored commit records an External entry"
    );
    // 5. POST-op push: the host already deleted B.
    protocol_seed_canonical_native(canon_payload_json(&["A"]), DOC.to_string())
        .expect("post-op push");
    assert_eq!(
        engine_layer_ids(DOC),
        vec!["A"],
        "the engine's layer vector followed the host's delete"
    );

    // 6. The host's undo: `facade.undo()` -> applyCommand({type:"undo"}).
    let undo = apply(DOC, r#"{"type":"undo"}"#);
    assert_eq!(
        undo["status"].as_str(),
        Some("external"),
        "the walker reports a host handoff it expects the host to confirm"
    );
    let seq = undo["externalSeq"]
        .as_u64()
        .expect("externalSeq for the commit");
    assert_eq!(
        delta_ids(&undo["delta"]["changes"]),
        vec!["+A", "+B"],
        "Rust restores the pre-delete vector on its own: a non-empty ordered restatement"
    );
    assert_eq!(
        engine_layer_ids(DOC),
        vec!["A", "B"],
        "the engine's layer vector actually changed - no host replay needed"
    );

    // 7. The host confirms, which moves the cursor and clears the barrier.
    let commit = protocol_history_cursor_commit_native(DOC.to_string(), seq, "undo".to_string())
        .expect("cursor commit");
    let committed: serde_json::Value = serde_json::from_str(&commit).expect("commit json");
    assert_eq!(committed["status"].as_str(), Some("external-confirmed"));

    // 8. The redo, the mirror of step 6.
    let redo = apply(DOC, r#"{"type":"redo"}"#);
    assert_eq!(redo["status"].as_str(), Some("external"));
    assert_eq!(
        delta_ids(&redo["delta"]["changes"]),
        vec!["-B", "+A"],
        "the redo re-removes exactly what the undo re-added"
    );
    assert_eq!(
        engine_layer_ids(DOC),
        vec!["A"],
        "the delete is back in force"
    );

    close_doc(DOC);
}

/// The gap counter, at the surface a caller can read it. `after` is None on
/// this stream because no post-op push ever landed, so the redo has no target;
/// the command still returns its handoff, and the engine counts the step rather
/// than reporting "nothing to restore" indistinguishably from a real no-op.
#[test]
fn an_uncaptured_post_state_is_counted_on_the_redo() {
    let _g = TEST_REGISTRY_LOCK.lock().unwrap();
    const DOC2: &str = "external-metadata-gap-count-wiring-doc";
    open_doc(DOC2);

    fn gaps(doc: &str) -> u64 {
        let reg = registry();
        reg.as_ref()
            .and_then(|r| r.docs.get(doc))
            .expect("open doc")
            .history
            .external_post_state_gaps()
    }

    protocol_seed_native(seed_payload_json(&["A", "B"]), DOC2.to_string()).expect("seed");
    protocol_register_adapter_native(DOC2.to_string(), "ts-external".to_string()).expect("adapter");
    // Pre-op push only. The post-op push never arrives, so `after` stays None.
    protocol_seed_canonical_native(canon_payload_json(&["A", "B"]), DOC2.to_string())
        .expect("pre-op push");
    let rec = record_external(DOC2, "Delete Layer", "tok-gap");
    let seq = rec["documentVersion"].as_u64();
    assert!(seq.is_some(), "the record bumped the document version");
    assert_eq!(gaps(DOC2), 0, "recording a step is not a gap");

    // The undo of an entry with no captured post-sync side restores nothing -
    // the engine's vector never moved across it - and that is NOT the gap the
    // counter tracks.
    let undo = apply(DOC2, r#"{"type":"undo"}"#);
    assert!(
        delta_ids(&undo["delta"]["changes"]).is_empty(),
        "no captured post state means no vector change to restate"
    );
    protocol_history_cursor_commit_native(DOC2.to_string(), 1, "undo".to_string()).expect("commit");
    assert_eq!(
        gaps(DOC2),
        0,
        "an undo with nothing to restate is not a gap"
    );

    // The redo is the direction that needs the post-sync side: it has no target.
    let redo = apply(DOC2, r#"{"type":"redo"}"#);
    assert_eq!(redo["status"].as_str(), Some("external"));
    assert!(
        delta_ids(&redo["delta"]["changes"]).is_empty(),
        "with no captured post state the redo can only hand the host its token"
    );
    assert_eq!(
        gaps(DOC2),
        1,
        "the redo counted itself instead of looking like a step that changed nothing"
    );

    // Control: the shipping sequence never reaches this counter, because the
    // post-op push fills the slot. Same document shape, one extra push.
    const DOC3: &str = "external-metadata-no-gap-wiring-doc";
    open_doc(DOC3);
    protocol_seed_native(seed_payload_json(&["A", "B"]), DOC3.to_string()).expect("seed");
    protocol_register_adapter_native(DOC3.to_string(), "ts-external".to_string()).expect("adapter");
    protocol_seed_canonical_native(canon_payload_json(&["A", "B"]), DOC3.to_string())
        .expect("pre-op push");
    record_external(DOC3, "Delete Layer", "tok-ok");
    protocol_seed_canonical_native(canon_payload_json(&["A"]), DOC3.to_string())
        .expect("post-op push");
    apply(DOC3, r#"{"type":"undo"}"#);
    protocol_history_cursor_commit_native(DOC3.to_string(), 1, "undo".to_string()).expect("commit");
    apply(DOC3, r#"{"type":"redo"}"#);
    // Scoped: `registry()` hands out a MutexGuard, and holding one across the
    // `close_doc` calls below would deadlock this thread on the same mutex.
    {
        let reg = registry();
        assert_eq!(
            reg.as_ref().unwrap().docs[DOC3]
                .history
                .external_post_state_gaps(),
            0,
            "the shipping sequence's redo had a target, so nothing was counted"
        );
    }

    close_doc(DOC2);
    close_doc(DOC3);
}
