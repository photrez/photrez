// Cursor-advance proof for the External-payload document-size restore, and the
// regression guard that the other entry kinds still move the shared cursor
// exactly as they did before this change.
//
// WHY THIS FILE EXISTS (the 6th-press refusal depends on it):
//
// The host refuses an undo/redo that has no user work behind it, and it reads
// the shared stream's cursor through `getHistoryQuery`
// (`facadeHistoryHandoff.ts:rustStreamHoldsUserWork`, lines 121-168):
// entries below the cursor, minus a one-entry document-open baseline floor.
// The undo button is enabled unconditionally under facade ownership, so a
// FROZEN cursor cannot surface as a dead button - the press dispatches,
// `history.undo()` returns null, and the app exits silently with no refusal
// log. The baseline floor is therefore only trustworthy while the cursor is
// PROVEN to advance for every entry kind that can sit at the tip.
//
// An External entry is the shape a host-handed crop records: the host applies
// its mutation out-of-band, so the walker cannot perform the step itself. The
// cursor moves on the two-phase cursor commit
// (`ProtocolEngine::history_cursor_commit`), NOT on the walker's early return.
// That two-phase split is the exact thing this change touched (the walker now
// also restores the document size before that early return), so the arithmetic
// is asserted end to end rather than assumed.
//
// Reading of the refusal, replicated for the assertions below:
//
//   undo:  work_below = cursor > floor        (floor = 1 iff baseline exists)
//   redo:  work_ahead = entries.len() > cursor
//
// The measured production sequence - one Pixel entry (a brush stroke) then one
// External entry (a host crop) - is walked press by press at the end, checking
// at every press that the stream keeps answering the refusal question and that
// each press consumes exactly one entry.

use super::*;
use crate::canonical_model::CanonicalDocument;
use crate::document_core::ProtocolEngine;
use crate::state_node::{StateMeta, TileRef};
use std::sync::Arc;

fn env(cmd: Command) -> CommandEnvelope {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: cmd,
    }
}

/// 1x1 `Arc<StateNode>` whose single RGBA pixel is filled with `v`.
fn sn(v: u8) -> Arc<StateNode> {
    let (w, h) = (1u32, 1u32);
    let data = vec![v; (w * h * 4) as usize];
    let tile = TileRef::owning(0, 0, w, h, &data);
    Arc::new(StateNode::new(0, vec![tile], StateMeta::new(w, h, 0, 0)))
}

fn add_layer(eng: &mut ProtocolEngine, id: &str) {
    eng.apply(env(Command::AddLayer {
        id: id.to_string(),
        name: id.into(),
        width: 100.0,
        height: 100.0,
        index: 0,
        layer_type: None,
        shape_params: None,
        text_data: None,
    }))
    .unwrap();
}

/// Open a document the way the host does: seed the canonical document (which is
/// the baseline `doc_size` owner) plus one layer, giving the one open-baseline
/// entry the refusal's floor accounts for.
fn seed_open_document(eng: &mut ProtocolEngine, w: f64, h: f64) {
    eng.seed_canonical(CanonicalDocument {
        id: "doc-1".into(),
        name: "doc".into(),
        width: w,
        height: h,
        layers: Vec::new(),
        selection: None,
    });
    add_layer(eng, "bg");
    assert_eq!(
        eng.doc_size(),
        Some((w, h)),
        "document open seeds the engine doc size"
    );
}

fn record_external(eng: &mut ProtocolEngine, label: &str, token: &str) {
    eng.apply(env(Command::RecordExternalTransition {
        label: label.into(),
        affected_layer_ids: vec!["bg".into()],
        adapter_id: "ts".into(),
        token: token.into(),
        memory_cost_bytes: 8,
    }))
    .unwrap();
}

/// The host's post-crop push of the NEW dimensions. This is the production path
/// that carries the cropped size into the entry's `doc_size_after` (see
/// `seed_canonical`). It deliberately does NOT move `self.doc_size`, which is
/// baseline-only by construction - the asymmetry the undo arm is written
/// against, and the reason the emission test below asserts the entry's halves
/// rather than the live field.
fn push_host_dims(eng: &mut ProtocolEngine, w: f64, h: f64) {
    eng.seed_canonical(CanonicalDocument {
        id: "doc-1".into(),
        name: "doc".into(),
        width: w,
        height: h,
        layers: Vec::new(),
        selection: None,
    });
}

/// The refusal's read, replicated: `cursor > floor` for undo,
/// `entries.len() > cursor` for redo. `floor` is the baseline the host knows
/// about, not something the stream can see, so it is passed in.
fn refusal_answers(eng: &ProtocolEngine, direction: &str, floor: u64) -> bool {
    let q = eng.history_query();
    match direction {
        "undo" => q.cursor > floor,
        _ => q.entries.len() as u64 > q.cursor,
    }
}

// ── The behaviour under proof ─────────────────────────────────────────────

/// A host crop records an External entry while the engine still holds the
/// PRE-crop size, then the host changes the document size out-of-band and
/// pushes it. Undoing the entry restores the pre-crop size on the delta the
/// host projects, and the cursor still advances.
#[test]
fn external_undo_restores_document_size_and_advances_cursor() {
    let mut eng = ProtocolEngine::new();
    seed_open_document(&mut eng, 128.0, 128.0);
    eng.register_adapter("ts");

    let baseline_entries = eng.history_query().entries.len() as u64;
    assert_eq!(
        baseline_entries, 1,
        "one open-baseline entry (the AddLayer)"
    );

    // The host crops: it commits its TS snapshot FIRST (before `applyCrop`
    // mutates anything), so the engine still holds the pre-crop size here.
    record_external(&mut eng, "Crop Canvas", "tok-crop");
    assert_eq!(eng.history_query().cursor, baseline_entries + 1);

    // The host's out-of-band crop, then its push of the new dimensions.
    push_host_dims(&mut eng, 13.0, 13.0);
    assert_eq!(
        eng.doc_size(),
        Some((128.0, 128.0)),
        "the host push must NOT move doc_size (baseline-only owner), so the \
         engine's live field is still pre-crop here"
    );

    // ── ONE undo press ──
    let cursor_before = eng.history_query().cursor;
    let r = eng.apply(env(Command::Undo)).unwrap();
    assert_eq!(r.status.as_deref(), Some("external"), "two-phase handoff");
    assert_eq!(r.external_seq, Some(2));
    assert_eq!(
        eng.history_query().cursor,
        cursor_before,
        "the walker early-return must NOT move the cursor"
    );

    // The restored size rides the delta the host projects. Asserted against the
    // ENTRY's halves, not against `self.doc_size` - which is still pre-crop
    // and would make a live-field comparison silently vacuous.
    assert_eq!(r.delta.width, Some(128.0), "delta carries pre-crop width");
    assert_eq!(r.delta.height, Some(128.0), "delta carries pre-crop height");

    // Host confirmation moves the cursor - the advance the refusal depends on.
    let c = eng.history_cursor_commit(2, "undo").unwrap();
    assert_eq!(c.status.as_deref(), Some("external-confirmed"));
    assert_eq!(
        eng.history_query().cursor,
        cursor_before - 1,
        "cursor advanced by exactly one on the commit"
    );

    // Symmetric: redo re-applies the crop's size.
    let r = eng.apply(env(Command::Redo)).unwrap();
    assert_eq!(r.external_seq, Some(2));
    assert_eq!(r.delta.width, Some(13.0), "redo re-applies cropped width");
    assert_eq!(r.delta.height, Some(13.0));
    eng.history_cursor_commit(2, "redo").unwrap();
    assert_eq!(
        eng.history_query().cursor,
        baseline_entries + 1,
        "redo advanced the cursor back"
    );
}

/// A metadata External entry (no document-size change - a layer delete, a move,
/// a reorder) must still emit a NULL-size delta exactly as before, so no
/// consumer keying on `width.is_some()` changes behaviour for the already-closed
/// paths.
#[test]
fn metadata_external_keeps_null_size_delta() {
    let mut eng = ProtocolEngine::new();
    seed_open_document(&mut eng, 128.0, 128.0);
    eng.register_adapter("ts");

    // No dimension change anywhere: the push carries the SAME size.
    record_external(&mut eng, "Delete Layer", "tok-del");
    push_host_dims(&mut eng, 128.0, 128.0);

    let r = eng.apply(env(Command::Undo)).unwrap();
    assert_eq!(r.status.as_deref(), Some("external"));
    assert_eq!(r.delta.width, None, "no size change -> no size delta");
    assert_eq!(r.delta.height, None);
}

// ── The regression guard: every other entry kind still advances ───────────

/// Native (Metadata) entries move the cursor in the walker itself.
#[test]
fn native_entry_still_advances_cursor_on_undo_and_redo() {
    let mut eng = ProtocolEngine::new();
    seed_open_document(&mut eng, 128.0, 128.0);

    add_layer(&mut eng, "a");
    assert_eq!(eng.history_query().cursor, 2);

    eng.apply(env(Command::Undo)).unwrap();
    assert_eq!(
        eng.history_query().cursor,
        1,
        "native undo moved the cursor"
    );
    eng.apply(env(Command::Redo)).unwrap();
    assert_eq!(
        eng.history_query().cursor,
        2,
        "native redo moved the cursor"
    );
}

/// Snapshot entries move the cursor through `undo_snapshot`/`redo_snapshot`.
/// They deliberately do NOT move it in the walker's Snapshot arm, so the caller
/// dispatches them - this asserts that contract is unchanged.
#[test]
fn snapshot_entry_still_advances_cursor_through_undo_snapshot() {
    let mut eng = ProtocolEngine::new();
    seed_open_document(&mut eng, 128.0, 128.0);

    let snap = |v: u64, w: u32| {
        crate::snapshot::DocumentSnapshot::new("doc-1", v)
            .with_layer(crate::snapshot::LayerSnapshot::new("bg", w, w))
    };
    eng.record_snapshot(snap(0, 128), snap(1, 13)).unwrap();
    assert_eq!(eng.history_query().cursor, 2);

    // The walker's Snapshot arm must NOT move the cursor (unchanged contract).
    eng.apply(env(Command::Undo)).unwrap();
    assert_eq!(
        eng.history_query().cursor,
        2,
        "walker leaves the Snapshot cursor alone"
    );

    // `undo_snapshot` is the dispatch that moves it.
    let got = eng.undo_snapshot().unwrap();
    assert!(got.is_some(), "undo_snapshot returned the before snapshot");
    assert_eq!(
        eng.history_query().cursor,
        1,
        "undo_snapshot moved the cursor"
    );

    let got = eng.redo_snapshot().unwrap();
    assert!(got.is_some());
    assert_eq!(
        eng.history_query().cursor,
        2,
        "redo_snapshot moved the cursor"
    );
}

/// Pixel entries move the cursor through the walker's Pixel arm.
#[test]
fn pixel_entry_still_advances_cursor_on_undo_and_redo() {
    let mut eng = ProtocolEngine::new();
    seed_open_document(&mut eng, 128.0, 128.0);

    eng.apply_pixel_patch("bg", sn(0), sn(7));
    assert_eq!(eng.history_query().cursor, 2);

    let (layer, tiles, _node) = eng.undo_pixel().unwrap();
    assert_eq!(layer.as_deref(), Some("bg"));
    assert_eq!(
        tiles.unwrap()[0].data[0],
        0,
        "undo restored the pre-stroke pixel"
    );
    assert_eq!(eng.history_query().cursor, 1, "pixel undo moved the cursor");

    let (_, tiles, _node) = eng.redo_pixel().unwrap();
    assert_eq!(tiles.unwrap()[0].data[0], 7, "redo re-applied the stroke");
    assert_eq!(eng.history_query().cursor, 2, "pixel redo moved the cursor");
}

/// The measured production sequence, walked press by press: a Pixel entry (a
/// brush stroke) then an External entry (a host crop). Asserts the cursor
/// relationship the 6th-press refusal reads at EVERY press, and that each
/// press consumes exactly one entry - the property whose loss would turn the
/// refusal into a silently dead undo button.
#[test]
fn measured_brush_then_crop_stream_answers_refusal_at_every_press() {
    let mut eng = ProtocolEngine::new();
    seed_open_document(&mut eng, 128.0, 128.0);
    eng.register_adapter("ts");

    // floor = 1: the host proved one open-baseline entry exists.
    let floor = 1u64;

    eng.apply_pixel_patch("bg", sn(0), sn(9));
    record_external(&mut eng, "Crop Canvas", "tok-crop");
    push_host_dims(&mut eng, 13.0, 13.0);

    assert_eq!(
        eng.history_query().entries.len(),
        3,
        "[baseline, pixel, external]"
    );
    assert_eq!(eng.history_query().cursor, 3);

    // Press 1: undo the crop. Work exists (cursor 3 > floor 1).
    assert!(
        refusal_answers(&eng, "undo", floor),
        "press 1 has work below"
    );
    let c0 = eng.history_query().cursor;
    let r = eng.apply(env(Command::Undo)).unwrap();
    assert_eq!(r.delta.width, Some(128.0), "press 1 restores the doc size");
    eng.history_cursor_commit(3, "undo").unwrap();
    assert_eq!(
        eng.history_query().cursor,
        c0 - 1,
        "press 1 consumed one entry"
    );
    assert!(
        refusal_answers(&eng, "undo", floor),
        "press 2 still has work below the baseline floor"
    );

    // Press 2: undo the stroke.
    let c1 = eng.history_query().cursor;
    eng.apply(env(Command::Undo)).unwrap();
    assert_eq!(
        eng.history_query().cursor,
        c1 - 1,
        "press 2 consumed one entry"
    );
    assert!(
        !refusal_answers(&eng, "undo", floor),
        "at the baseline floor the refusal answers correctly (press 3 refused)"
    );

    // Press 3: the host gate refuses it, so no dispatch happens in production.
    // Asserted here by DISPATCHING anyway, to pin the reason the gate has to
    // exist: the engine itself has no baseline-floor guard and will happily walk
    // the cursor past the floor and undo the document-open entry. That is
    // precisely why the refusal must be a host-side read of this cursor - and
    // why a frozen cursor would silently convert the refusal into a live undo
    // button that destroys the baseline instead of refusing.
    assert!(
        !refusal_answers(&eng, "undo", floor),
        "the host gate is what refuses press 3"
    );
    let c2 = eng.history_query().cursor;
    assert_eq!(c2, floor, "cursor sits exactly on the baseline floor");
    eng.apply(env(Command::Undo)).unwrap();
    assert_eq!(
        eng.history_query().cursor,
        c2 - 1,
        "the ENGINE has no floor guard - it undoes the baseline entry, which \
         is exactly why the host must refuse this press from the cursor alone"
    );

    // Redo direction answers from entries AHEAD of the cursor.
    assert!(refusal_answers(&eng, "redo", floor), "redo has work ahead");
}
