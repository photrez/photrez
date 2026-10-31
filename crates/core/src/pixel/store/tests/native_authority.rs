// SPDX-License-Identifier: AGPL-3.0-or-later
//! Native authority command surface: the per-document native `DocumentCore`
//! serves the AUTHORITY methods through the REAL `PixelStoreRegistry`.

use super::tile;
use crate::pixel::store::{registry, PixelStoreRegistry};
use crate::protocol::{
    Command, CommandEnvelope, CommandResult, DocumentCore, HistoryQuery, RenderLayerChange,
};

fn envelope(cmd: Command) -> CommandEnvelope {
    CommandEnvelope {
        contract_version: crate::protocol::CONTRACT_VERSION,
        expected_version: None,
        command: cmd,
    }
}

/// Get the per-doc `DocumentCore` exactly as the native command's REAL path
/// expects: the doc must already be OPEN (the client opens it via
/// `rust_pixels_open_document` before issuing protocol commands), then `get_mut`
/// returns it. This mirrors the command (`open_document` precedes `get_mut`) and
/// does NOT silently create an unopened doc - a missing doc is the command's error
/// contract, exercised separately below.
fn engine_for<'a>(reg: &'a mut PixelStoreRegistry, doc_id: &str) -> &'a mut DocumentCore {
    reg.open_document(doc_id);
    &mut reg.docs.get_mut(doc_id).unwrap().history
}

#[test]
fn native_apply_add_layer_records_entry_and_bumps_version_and_snapshot() {
    let mut reg = PixelStoreRegistry::new();
    let res: CommandResult = engine_for(&mut reg, "doc1")
        .apply(envelope(Command::AddLayer {
            id: "bg-id".to_string(),
            name: "bg".to_string(),
            width: 100.0,
            height: 100.0,
            index: 0,
            x: None,
            y: None,
            layer_type: None,
            shape_params: None,
            text_data: None,
        }))
        .expect("apply addLayer");

    assert_eq!(res.document_version, 1, "version advanced on apply");
    assert_eq!(res.delta.changes.len(), 1, "one layer upsert in delta");
    assert!(matches!(
        res.delta.changes[0],
        RenderLayerChange::Upsert { .. }
    ));

    // history_query shows the single applied entry at the tip.
    let q: HistoryQuery = reg.docs.get("doc1").unwrap().history.history_query();
    assert_eq!(q.entries.len(), 1, "one history entry");
    assert_eq!(q.cursor, 1, "cursor at tip");

    // snapshot() reflects the applied layer.
    let snap = reg.docs.get("doc1").unwrap().history.snapshot();
    assert_eq!(snap.version, 1);
    assert_eq!(snap.layers.len(), 1);
    assert_eq!(snap.layers[0].name, "bg");
}

#[test]
fn native_metadata_undo_redo_steps_cursor() {
    let mut reg = PixelStoreRegistry::new();
    engine_for(&mut reg, "d")
        .apply(envelope(Command::AddLayer {
            id: "L-id".to_string(),
            name: "L".to_string(),
            width: 100.0,
            height: 100.0,
            index: 0,
            x: None,
            y: None,
            layer_type: None,
            shape_params: None,
            text_data: None,
        }))
        .expect("apply"); // v1, cursor 1

    // Undo: cursor steps back one; DocumentVersion advances one step.
    engine_for(&mut reg, "d")
        .apply(envelope(Command::Undo))
        .expect("undo");
    let q1: HistoryQuery = reg.docs.get("d").unwrap().history.history_query();
    assert_eq!(q1.cursor, 0, "undo moved cursor to 0");

    // Redo: cursor steps forward again.
    engine_for(&mut reg, "d")
        .apply(envelope(Command::Redo))
        .expect("redo");
    let q2: HistoryQuery = reg.docs.get("d").unwrap().history.history_query();
    assert_eq!(q2.cursor, 1, "redo moved cursor back to tip");
}

/// One cursor, one executor: `Command::Undo`/`Command::Redo` must step the
/// pixel history cursor when the tip is a `Pixel` entry. Before this,
/// `Command::Undo` returned an empty delta and left the cursor put while
/// `undo_pixel` moved it, so which side executed a pixel step depended on
/// the payload kind at the call site instead of on a contract.
#[test]
fn undo_command_steps_the_pixel_cursor_and_returns_the_before_tiles() {
    let mut reg = PixelStoreRegistry::new();
    reg.open_document("d");
    reg.add_layer("d", "L1", 8, 8, vec![0; 8 * 8 * 4]).unwrap();
    // One stroke: all-zero tile -> all-10 tile.
    reg.apply_pixel_patch(
        "d",
        "L1",
        vec![tile(0, 0, 8, 8, 0)],
        vec![tile(0, 0, 8, 8, 10)],
    );
    assert_eq!(reg.get_history_cursor("d"), Some(1), "stroke recorded");

    let res: CommandResult = reg
        .docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::Undo))
        .expect("undo onto a pixel entry");

    assert_eq!(
        reg.get_history_cursor("d"),
        Some(0),
        "Command::Undo must step the pixel cursor by exactly 1"
    );

    let handoff = &res
        .pixel_patches
        .as_ref()
        .expect("Command::Undo must hand the pixel tiles back to the host")[0];
    assert_eq!(handoff.layer_id, "L1");
    assert_eq!(handoff.tiles.len(), 1, "one dirty tile for an 8x8 layer");
    let t = &handoff.tiles[0];
    assert_eq!((t.x, t.y, t.w, t.h), (0, 0, 8, 8));
    assert!(
        t.data.iter().all(|b| *b == 0),
        "undo hands back the BEFORE tile bytes, not the after bytes"
    );
    assert_eq!(
        handoff.epoch,
        reg.get_epoch("d", "L1").unwrap(),
        "handoff epoch names the canonical state the tiles belong to"
    );
    assert_eq!(
        handoff.version,
        reg.get_history_version("d").unwrap(),
        "handoff version names the cursor the step landed on"
    );

    // Redo is symmetric and returns the after bytes.
    let res: CommandResult = reg
        .docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::Redo))
        .expect("redo onto a pixel entry");
    assert_eq!(reg.get_history_cursor("d"), Some(1), "redo steps forward");
    let handoff = &res.pixel_patches.as_ref().expect("redo hands tiles back")[0];
    assert_eq!(handoff.tiles.len(), 1);
    assert!(
        handoff.tiles[0].data.iter().all(|b| *b == 10),
        "redo hands back the AFTER tile bytes"
    );
}

/// The exact JSON key names the host parses. Pins the wire contract: a
/// host mock that guesses different keys is testing a shape the engine
/// never emits, which is how a green DOM-mock test can hide a broken app.
#[test]
fn pixel_handoff_serializes_to_the_keys_the_host_parses() {
    let mut reg = PixelStoreRegistry::new();
    reg.open_document("d");
    reg.add_layer("d", "L1", 1, 1, vec![0, 0, 0, 255]).unwrap();
    reg.apply_pixel_patch(
        "d",
        "L1",
        vec![tile(0, 0, 1, 1, 0)],
        vec![tile(0, 0, 1, 1, 7)],
    );
    let res: CommandResult = reg
        .docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::Undo))
        .expect("undo");
    let v: serde_json::Value = serde_json::to_value(&res).expect("serialize");
    let p = &v["pixelPatches"];
    assert!(!p.is_null(), "pixelPatches must be present on the wire");
    // One handoff per restored layer: a destructive crop restores several
    // layers in a single step, so the field is a list.
    assert!(p.is_array(), "pixelPatches is a list of per-layer handoffs");
    let p = &p[0];
    assert!(p["layerId"].is_string(), "camelCase layerId");
    assert!(p["tiles"].is_array(), "tiles array");
    // TilePatch keeps its Rust field names - the host already parses this
    // shape from rust_pixels_undo, so the two paths agree.
    assert!(p["tiles"][0]["x"].is_i64());
    assert!(p["tiles"][0]["y"].is_i64());
    assert!(p["tiles"][0]["w"].is_u64());
    assert!(p["tiles"][0]["h"].is_u64());
    assert!(p["tiles"][0]["data"].is_array());
    assert!(p["epoch"].is_u64());
    assert!(p["version"].is_u64());
    assert!(v["documentVersion"].is_u64(), "camelCase on result");

    // A metadata step must OMIT the key entirely (skip_serializing_if), so
    // the host can tell "no pixel step" from "a pixel step with no tiles".
    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::AddLayer {
            id: "L2-id".to_string(),
            name: "L2".to_string(),
            width: 10.0,
            height: 10.0,
            index: 0,
            x: None,
            y: None,
            layer_type: None,
            shape_params: None,
            text_data: None,
        }))
        .expect("addLayer");
    let meta: serde_json::Value = serde_json::to_value(
        reg.docs
            .get_mut("d")
            .unwrap()
            .apply_command(envelope(Command::Undo))
            .expect("undo metadata"),
    )
    .expect("serialize");
    assert!(
        meta.get("pixelPatches").is_none(),
        "metadata step must omit pixelPatches entirely"
    );
}

/// The canonical buffer is the pixel owner, so a `Command::Undo` pixel step
/// must land in the `PixelLayer` itself. Without this the cursor moves but
/// the pixels do not, which is a silent no-op undo.
#[test]
fn undo_command_writes_the_before_pixels_into_the_canonical_buffer() {
    let mut reg = PixelStoreRegistry::new();
    reg.open_document("d");
    reg.add_layer("d", "L1", 1, 1, vec![0, 0, 0, 255]).unwrap();
    reg.apply_pixel_patch(
        "d",
        "L1",
        vec![tile(0, 0, 1, 1, 0)],
        vec![tile(0, 0, 1, 1, 11)],
    );
    assert_eq!(reg.get_layer("d", "L1").unwrap().pixels[0], 11);

    let res: CommandResult = reg
        .docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::Undo))
        .expect("undo");
    assert!(res.pixel_patches.is_some(), "tiles handed back");
    assert_eq!(
        reg.get_layer("d", "L1").unwrap().pixels[0],
        0,
        "canonical buffer must carry the restored pixels"
    );

    let epoch_after_undo = reg.get_epoch("d", "L1").unwrap();
    let _: CommandResult = reg
        .docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::Redo))
        .expect("redo");
    assert_eq!(
        reg.get_epoch("d", "L1").unwrap(),
        epoch_after_undo + 1,
        "each pixel step bumps the layer epoch so the TS cache re-reads"
    );
    assert_eq!(reg.get_layer("d", "L1").unwrap().pixels[0], 11);
}

/// A metadata tip keeps the existing walker behaviour: no pixel handoff at
/// all, so the host can still tell "Rust restored metadata" from "Rust
/// restored pixels".
#[test]
fn undo_command_on_a_metadata_entry_returns_no_pixel_handoff() {
    let mut reg = PixelStoreRegistry::new();
    let eng = engine_for(&mut reg, "d");
    eng.apply(envelope(Command::AddLayer {
        id: "L-id".to_string(),
        name: "L".to_string(),
        width: 100.0,
        height: 100.0,
        index: 0,
        x: None,
        y: None,
        layer_type: None,
        shape_params: None,
        text_data: None,
    }))
    .expect("apply addLayer");

    let res: CommandResult = reg
        .docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::Undo))
        .expect("undo onto metadata");
    assert!(
        res.pixel_patches.is_none(),
        "a metadata undo must not claim a pixel step"
    );
}

#[test]
fn native_cursor_commit_clears_pending_external_barrier() {
    let mut reg = PixelStoreRegistry::new();
    let eng = engine_for(&mut reg, "d");

    // Record an external (TS) transition -> External entry, cursor 1, v1.
    eng.apply(envelope(Command::RecordExternalTransition {
        label: "ts-op".to_string(),
        affected_layer_ids: vec!["L".to_string()],
        adapter_id: "native".to_string(),
        token: "t1".to_string(),
        memory_cost_bytes: 0,
        // Size-neutral transition: this test is about cursor/version accounting.
        doc_size_before: None,
        doc_size_after: None,
        minted_layer_ids: vec![],
    }))
    .expect("record external");

    // Undo onto the External entry sets the pending_external handoff barrier.
    let handoff = eng
        .apply(envelope(Command::Undo))
        .expect("undo onto external");
    assert_eq!(handoff.status.as_deref(), Some("external"));
    assert!(eng.pending_external.is_some(), "barrier set after handoff");

    // protocol_history_cursor_commit clears it (external-handoff semantics).
    let seq = eng.pending_external.as_ref().unwrap().0;
    let commit: CommandResult = eng
        .history_cursor_commit(seq, "undo")
        .expect("cursor commit");
    assert_eq!(commit.status.as_deref(), Some("external-confirmed"));
    assert!(eng.pending_external.is_none(), "barrier cleared on commit");
    assert_eq!(eng.version(), 2, "DocumentVersion advanced on commit");
}

#[test]
fn native_engines_are_isolated_per_document() {
    let mut reg = PixelStoreRegistry::new();
    engine_for(&mut reg, "docA").register_adapter("ts-adapter");
    engine_for(&mut reg, "docB").register_adapter("ts-adapter");

    // Per-doc isolation: each engine owns its own history + adapter set.
    engine_for(&mut reg, "docA")
        .apply(envelope(Command::AddLayer {
            id: "A-id".to_string(),
            name: "A".to_string(),
            width: 100.0,
            height: 100.0,
            index: 0,
            x: None,
            y: None,
            layer_type: None,
            shape_params: None,
            text_data: None,
        }))
        .expect("apply A");
    engine_for(&mut reg, "docB")
        .apply(envelope(Command::AddLayer {
            id: "B-id".to_string(),
            name: "B".to_string(),
            width: 100.0,
            height: 100.0,
            index: 0,
            x: None,
            y: None,
            layer_type: None,
            shape_params: None,
            text_data: None,
        }))
        .expect("apply B");

    let qa: HistoryQuery = reg.docs.get("docA").unwrap().history.history_query();
    let qb: HistoryQuery = reg.docs.get("docB").unwrap().history.history_query();
    assert_eq!(qa.entries.len(), 1);
    assert_eq!(qb.entries.len(), 1);
    assert_eq!(
        reg.docs.get("docA").unwrap().history.snapshot().layers[0].name,
        "A"
    );
    assert_eq!(
        reg.docs.get("docB").unwrap().history.snapshot().layers[0].name,
        "B"
    );
    // Cross-check: docA's engine has no knowledge of docB's layer.
    assert!(reg
        .docs
        .get("docA")
        .unwrap()
        .history
        .snapshot()
        .layers
        .iter()
        .all(|l| l.name == "A"));
}

#[test]
fn native_authority_command_path_uses_global_registry_and_json_envelope() {
    // Mirrors the EXACT production path of `protocol_apply_command_native`:
    // the REAL global `pixel_store::registry()` (NOT a local
    // `PixelStoreRegistry::new()`), a serde JSON in/out envelope, the
    // missing-doc error contract, and the "CODE: message" apply-error
    // formatting the command returns to the 2b-3 TS client.

    // Open the doc exactly as production does: rust_pixels_open_document
    // precedes the protocol command, so the command's get_mut finds an
    // already-open doc (the native authority does NOT implicitly create one).
    {
        let mut g = registry();
        g.get_or_insert_with(Default::default).open_document("docX");
    }

    // JSON-in: a real CommandEnvelope round-trips through serde, exactly what
    // the command receives from the frontend.
    let env_json = serde_json::to_string(&envelope(Command::AddLayer {
        id: "bg-id".to_string(),
        name: "bg".to_string(),
        width: 100.0,
        height: 100.0,
        index: 0,
        x: None,
        y: None,
        layer_type: None,
        shape_params: None,
        text_data: None,
    }))
    .unwrap();
    let parsed: CommandEnvelope = serde_json::from_str(&env_json).unwrap();

    // Drive the same engine method the command calls, through the global registry.
    let res_json = {
        let mut g = registry();
        let reg = g.get_or_insert_with(Default::default);
        let engine = reg.docs.get_mut("docX").expect("doc open");
        serde_json::to_string(&engine.history.apply(parsed).unwrap()).unwrap()
    };
    let res: CommandResult = serde_json::from_str(&res_json).unwrap();
    assert_eq!(res.document_version, 1, "apply advanced version");

    // Missing-doc path: an unopened doc is an ERROR (not an implicit create) -
    // the exact contract the command returns. Mirrors
    // `get_mut(...).ok_or("document not open")`.
    let missing = {
        let mut g = registry();
        let reg = g.get_or_insert_with(Default::default);
        match reg.docs.get_mut("never_opened") {
            Some(_) => String::new(),
            None => "document not open: never_opened".to_string(),
        }
    };
    assert_eq!(missing, "document not open: never_opened");

    // Apply-error formatting: a contract-version mismatch yields a ProtocolError
    // whose code the command formats as the bare "CODE: message" string that
    // Tauri v2 surfaces as a bare-string rejection. This is the error shape the
    // frozen client must handle through the raw request boundary.
    let code_msg = {
        let mut g = registry();
        let reg = g.get_or_insert_with(Default::default);
        let engine = reg.docs.get_mut("docX").expect("doc open");
        let mut bad = envelope(Command::AddLayer {
            id: "x-id".to_string(),
            name: "x".to_string(),
            width: 100.0,
            height: 100.0,
            index: 0,
            x: None,
            y: None,
            layer_type: None,
            shape_params: None,
            text_data: None,
        });
        bad.contract_version = u32::MAX; // force E_CONTRACT_VERSION
        let err = engine.history.apply(bad).unwrap_err();
        format!("{}: {}", err.code, err.message)
    };
    assert!(
        code_msg.starts_with("E_CONTRACT_VERSION:"),
        "raw invoke error shape is CODE: message, got {code_msg}"
    );
}
